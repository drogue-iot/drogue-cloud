
#
# By default, build and push artifacts and containers.
#
all: build images test push

CURRENT_DIR ?= $(strip $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST)))))
TOP_DIR ?= $(CURRENT_DIR)
IMAGE_TAG ?= latest
BUILDER_IMAGE ?= ghcr.io/drogue-iot/builder:0.1.1

MODULE:=$(basename $(shell realpath --relative-to $(TOP_DIR) $(CURRENT_DIR)))

ifneq ($(MODULE),)
	IMAGES=$(MODULE)
endif


#
# all container images that we build and push (so it does not include the "builder")
#
IMAGES?=\
	http-endpoint \
	mqtt-endpoint \
	console-backend \
	console-frontend \
	authentication-service \
	device-management-service \
	database-migration \
	command-endpoint \


#
# Restore a clean environment.
#
clean:
	cargo clean
	rm -Rf .cargo-container-home


#
# Build artifacts and containers.
#
build: host-build


#
# Run all tests.
#
test: host-test


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
# Run a build on the host, forking off into the build container.
#
host-build:
	docker run --rm -t -v "$(TOP_DIR):/usr/src:z" "$(BUILDER_IMAGE)" make -j1 -C /usr/src/$(MODULE) container-build


#
# Run tests on the host, forking off into the build container.
#
host-test:
	docker run --rm -t -v "$(TOP_DIR):/usr/src:z" "$(BUILDER_IMAGE)" make -j1 -C /usr/src/$(MODULE) container-test


#
# Change the permissions from inside the build container. Required for GitHub Actions, to make the build artifacts
# accessible the build runner.
#
fix-permissions:
	docker run --rm -t -v "$(TOP_DIR):/usr/src:z" -e FIX_UID="$(shell id -u)" "$(BUILDER_IMAGE)" bash -c 'chown $${FIX_UID} -R $${CARGO_HOME} /usr/src/target'


#
# Run an interactive shell inside the build container.
#
build-shell:
	docker run --rm -it -v "$(CURRENT_DIR):/usr/src:z" -e FIX_UID="$(shell id -u)" "$(BUILDER_IMAGE)" bash


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
	cd $(TOP_DIR) && docker build . -f $%/Dockerfile -t $%:$(IMAGE_TAG)


#
# Tag Images.
#
tag-images: tag-image($(IMAGES))
tag-image($(IMAGES)): require-container-registry
	cd $(TOP_DIR) && docker tag $% $(CONTAINER_REGISTRY)/$%:$(IMAGE_TAG)


#
# Push images.
#
push-images: push-image($(IMAGES))
push-image($(IMAGES)): require-container-registry
	cd $(TOP_DIR) && docker push $(CONTAINER_REGISTRY)/$%:$(IMAGE_TAG)


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
	./hack/drogue.sh -d build/deploy

gen-deploy: require-container-registry
	rm -Rf build
	mkdir -p build
	./hack/replace-images.py latest Always deploy "$(CONTAINER_REGISTRY)" build/deploy


#
# Check if we have a container registry set.
#
require-container-registry:
ifndef CONTAINER_REGISTRY
	$(error CONTAINER_REGISTRY is undefined)
endif


.PHONY: all clean build test push images
.PHONY: require-container-registry
.PHONY: deploy gen-deploy

.PHONY: build-images tag-images push-images
.PHONY: build-image($(IMAGES)) tag-image($(IMAGES)) push-image($(IMAGES))

.PHONY: container-build container-test
.PHONY: host-build host-test
.PHONY: cargo-build cargo-test