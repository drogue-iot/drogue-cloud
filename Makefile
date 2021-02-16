
#
# By default, build and push artifacts and containers.
#
all: build test

CURRENT_DIR ?= $(strip $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST)))))
TOP_DIR ?= $(CURRENT_DIR)
IMAGE_TAG ?= latest
BUILDER_IMAGE ?= ghcr.io/drogue-iot/builder:0.1.5

MODULE:=$(basename $(shell realpath --relative-to $(TOP_DIR) $(CURRENT_DIR)))

ifneq ($(MODULE),)
	IMAGES=$(MODULE)
endif

CONTAINER ?= docker
ifeq ($(CONTAINER),docker)
TEST_CONTAINER_ARGS ?= -v /var/run/docker.sock:/var/run/docker.sock:z --network drogue
endif
ifeq ($(CONTAINER),podman)
TEST_CONTAINER_ARGS ?= --security-opt label=disable -v $(XDG_RUNTIME_DIR)/podman/podman.sock:/var/run/docker.sock:z
endif

#
# all possible container images that we build and push (so it does not include the "builder")
#
ALL_IMAGES=\
	http-endpoint \
	mqtt-endpoint \
	console-backend \
	console-frontend \
	authentication-service \
	device-management-service \
	database-migration \
	command-endpoint \
	test-cert-generator \

#
# Active images to build
#
IMAGES ?= $(ALL_IMAGES)

#
# Restore a clean environment.
#
clean:
	cargo clean
	rm -Rf .cargo-container-home


#
# Check the code
#
check: host-check


#
# Build artifacts and containers.
#
build: host-build build-images


#
# Run all tests.
#
test: host-test


#
# Run checks on the source code
#
container-check: cargo-check


#
# Invoked inside the container, by a call to `host-build`.
#
# If you have the same environment as the build container, you can also run this on the host, instead of `host-build`.
#
container-build: cargo-build
ifeq ($(MODULE),)
container-build: webpack-build
endif


#
# Invoked inside the container, by a call to `host-test`.
#
# If you have the same environment as the build container, you can also run this on the host, instead of `host-test`.
#
container-test: cargo-test


#
# Run checks on the host, forking off into the build container.
#
host-check:
	$(CONTAINER) run --rm -t -v "$(TOP_DIR):/usr/src:z" "$(BUILDER_IMAGE)" make -j1 -C /usr/src/$(MODULE) container-check


#
# Run a build on the host, forking off into the build container.
#
host-build:
	$(CONTAINER) run --rm -t -v "$(TOP_DIR):/usr/src:z" "$(BUILDER_IMAGE)" make -j1 -C /usr/src/$(MODULE) container-build


#
# Run tests on the host, forking off into the build container.
#
host-test:
	if [ -z "$$($(CONTAINER) network ls --format '{{.Name}}' | grep drogue)" ]; then $(CONTAINER) network create drogue; fi
	$(CONTAINER) run --rm -t -v "$(TOP_DIR):/usr/src:z" $(TEST_CONTAINER_ARGS) "$(BUILDER_IMAGE)" make -j1 -C /usr/src/$(MODULE) container-test


#
# Change the permissions from inside the build container. Required for GitHub Actions, to make the build artifacts
# accessible the build runner.
#
fix-permissions:
	$(CONTAINER) run --rm -t -v "$(TOP_DIR):/usr/src:z" -e FIX_UID="$(shell id -u)" "$(BUILDER_IMAGE)" bash -c 'chown $${FIX_UID} -R $${CARGO_HOME} /usr/src/target'


#
# Run an interactive shell inside the build container.
#
build-shell:
	$(CONTAINER) run --rm -it -v "$(CURRENT_DIR):/usr/src:z" -e FIX_UID="$(shell id -u)" "$(BUILDER_IMAGE)" bash


#
# Check the code
#
cargo-check:
	cargo fmt --all -- --check
	cargo check --release
	cargo clippy --release --all-features


#
# Run the cargo build.
#
cargo-build:
	@#
	@# We build everything, expect the wasm stuff. Wasm will be compiled in a separate step, and we don't need
	@# the build to compile all the dependencies, which we only use in wasm, for the standard target triple.
	@#
	cargo build --release --workspace --exclude drogue-cloud-console-frontend


#
# Run the cargo tests.
#
cargo-test:
	cargo test --release -- $(CARGO_TEST_OPTS)


#
# Run the webpack build.
#
webpack-build: cargo-build
	cd console-frontend && npm install
	cd console-frontend && npm run build


#
# Build images.
#
# You might want to consider doing a `build` first, but we don't force that.
#
build-images: build-image($(IMAGES))
build-image($(IMAGES)):
	cd $(TOP_DIR) && $(CONTAINER) build . -f $%/Dockerfile -t $%:$(IMAGE_TAG)


#
# Tag Images.
#
tag-images: tag-image($(IMAGES))
tag-image($(IMAGES)): require-container-registry
	cd $(TOP_DIR) && $(CONTAINER) tag $% $(CONTAINER_REGISTRY)/$%:$(IMAGE_TAG)


#
# Push images.
#
push-images: push-image($(IMAGES))
push-image($(IMAGES)): require-container-registry
	cd $(TOP_DIR) && $(CONTAINER) push $(CONTAINER_REGISTRY)/$%:$(IMAGE_TAG)


#
# Save all images.
#
save-images: require-container-registry
	mkdir -p "$(TOP_DIR)/build/images"
	rm -Rf "$(TOP_DIR)/build/images/all.tar"
	$(CONTAINER) save -o "$(TOP_DIR)/build/images/all.tar" $(addprefix $(CONTAINER_REGISTRY)/, $(addsuffix :$(IMAGE_TAG), $(IMAGES)))


#
# Tag and push images.
#
push: tag-images push-images


#
# Build and push images.
#
images: build-images tag-images push-images


#
# Quick local build without tests and pushing images
#
quick: build build-images tag-images


#
# Do a local deploy
#
deploy: gen-deploy
	env TEST_CERTS_IMAGE=$(CONTAINER_REGISTRY)/test-cert-generator:latest ./hack/drogue.sh -d build/deploy

gen-deploy: require-container-registry
	rm -Rf build/deploy
	mkdir -p build/deploy
	./hack/replace-images.py latest Always deploy "$(CONTAINER_REGISTRY)" build/deploy


#
# Check if we have a container registry set.
#
require-container-registry:
ifndef CONTAINER_REGISTRY
	$(error CONTAINER_REGISTRY is undefined)
endif


.PHONY: all clean check build test push images
.PHONY: require-container-registry
.PHONY: deploy gen-deploy

.PHONY: build-images tag-images push-images
.PHONY: build-image($(IMAGES)) tag-image($(IMAGES)) push-image($(IMAGES))

.PHONY: save-images

.PHONY: container-check container-build container-test
.PHONY: host-check host-build host-test
.PHONY: cargo-check cargo-build cargo-test