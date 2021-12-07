
#
# By default, build and push artifacts and containers.
#
all: build test

CURRENT_DIR ?= $(strip $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST)))))
TOP_DIR ?= $(CURRENT_DIR)
IMAGE_TAG ?= latest
BUILDER_IMAGE ?= ghcr.io/drogue-iot/builder:0.1.19
SKIP_SERVER ?= false

MODULE:=$(basename $(shell realpath --relative-to $(TOP_DIR) $(CURRENT_DIR)))

ifneq ($(MODULE),)
	IMAGES=$(MODULE)
endif

# evaluate which container tool we use
ifeq (, $(shell which podman 2>/dev/null))
	CONTAINER ?= docker
else
	CONTAINER ?= podman
endif

# evaluate the arguments we need for this container tool
ifeq ($(CONTAINER),docker)
	TEST_CONTAINER_ARGS ?= -v /var/run/docker.sock:/var/run/docker.sock:z --network drogue
	CONTAINER_ARGS ?= -u "$(shell id -u):$(shell id -g)" $(patsubst %,--group-add %,$(shell id -G ))
else ifeq ($(CONTAINER),podman)
	TEST_CONTAINER_ARGS ?= --security-opt label=disable -v $(XDG_RUNTIME_DIR)/podman/podman.sock:/var/run/docker.sock:z
	CONTAINER_ARGS ?= --userns=keep-id
endif


#
# all possible container images that we build and push (so it does not include the "builder")
#
ALL_IMAGES=\
	coap-endpoint \
	http-endpoint \
	mqtt-endpoint \
	console-backend \
	console-frontend \
	authentication-service \
	device-management-service \
	database-migration \
	command-endpoint \
	test-cert-generator \
	outbox-controller \
	user-auth-service \
	mqtt-integration \
	ttn-operator \
	topic-strimzi-operator \
	topic-admin-operator \
	websocket-integration \
	ditto-registry-operator \



# allow skipping the server image
ifeq ($(SKIP_SERVER), false)
ALL_IMAGES += server
endif


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
# Pre-check the code, just check checks
#
pre-check: host-pre-check


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
# Run pre-checks on the source code
#
container-pre-check: cargo-pre-check


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
container-build: frontend-build
endif


#
# Invoked inside the container, by a call to `host-test`.
#
# If you have the same environment as the build container, you can also run this on the host, instead of `host-test`.
#
container-test: cargo-test


#
# Run pre-checks on the host, forking off into the build container.
#
host-pre-check:
	$(CONTAINER) run $(CONTAINER_ARGS) --rm -t -v "$(TOP_DIR):/usr/src:z" "$(BUILDER_IMAGE)" make -j1 -C /usr/src/$(MODULE) container-pre-check


#
# Run checks on the host, forking off into the build container.
#
host-check:
	$(CONTAINER) run $(CONTAINER_ARGS) --rm -t -v "$(TOP_DIR):/usr/src:z" "$(BUILDER_IMAGE)" make -j1 -C /usr/src/$(MODULE) container-check


#
# Run a build on the host, forking off into the build container.
#
host-build:
	$(CONTAINER) run $(CONTAINER_ARGS) --rm -t -v "$(TOP_DIR):/usr/src:z" "$(BUILDER_IMAGE)" make -j1 -C /usr/src/$(MODULE) container-build SKIP_SERVER=$(SKIP_SERVER)


#
# Run tests on the host, forking off into the build container.
#
host-test:
	if [ -z "$$($(CONTAINER) network ls --format '{{.Name}}' | grep drogue)" ]; then $(CONTAINER) network create drogue; fi
	$(CONTAINER) run $(CONTAINER_ARGS) --rm -t \
		$${RUST_LOG+-e RUST_LOG=$${RUST_LOG}} $${RUST_BACKTRACE+-e RUST_BACKTRACE=$${RUST_BACKTRACE}} \
		-v "$(TOP_DIR):/usr/src:z" $(TEST_CONTAINER_ARGS) "$(BUILDER_IMAGE)" \
		make -j1 CONTAINER=$(CONTAINER) -C /usr/src/$(MODULE) container-test



#
# Run an interactive shell inside the build container.
#
build-shell:
	$(CONTAINER) run $(CONTAINER_ARGS) --rm -ti -v "$(CURRENT_DIR):/usr/src:z" -e FIX_UID="$(shell id -u)" "$(BUILDER_IMAGE)" bash


#
# Run an interactive shell inside the build container, like for testing.
#
.PHONY: test-shell
test-shell:
	$(CONTAINER) run $(CONTAINER_ARGS) --rm -t \
		$${RUST_LOG+-e RUST_LOG=$${RUST_LOG}} $${RUST_BACKTRACE+-e RUST_BACKTRACE=$${RUST_BACKTRACE}} \
		-v "$(TOP_DIR):/usr/src:z" $(TEST_CONTAINER_ARGS) "$(BUILDER_IMAGE)" \
		bash


#
# Pre-check code
#
cargo-pre-check:
	cargo fmt --all -- --check


#
# Check the code
#
cargo-check: cargo-pre-check cargo-check-frontend
	cargo check --release
	cargo clippy --release --all-features


#
# Check the frontend project
#
.PHONY: cargo-check-frontend
cargo-check-frontend:
	cd console-frontend && cargo check --release
	cd console-frontend && cargo clippy --release --all-features


#
# Run the cargo build.
#
ifneq ($(MODULE),)
cargo-build: CARGO_BUILD_ARGS := --package drogue-cloud-$(MODULE)
endif

ifneq ($(SKIP_SERVER), false)
cargo-build: CARGO_BUILD_ARGS := --exclude drogue-cloud-server $(CARGO_BUILD_ARGS)
endif

cargo-build:
	@#
	@# We build everything, expect the wasm stuff. Wasm will be compiled in a separate step, and we don't need
	@# the build to compile all the dependencies, which we only use in wasm, for the standard target triple.
	@#
	cargo build --release --workspace $(CARGO_BUILD_ARGS)


#
# Run the cargo tests.
#
cargo-test:
	cargo test --release -- $(CARGO_TEST_OPTS)


#
# Run the frontend build.
#
frontend-build: cargo-build
	cd console-frontend && npm install
	cd console-frontend && trunk build --release


#
# Build images.
#
# You might want to consider doing a `build` first, but we don't enforce that.
#
build-images: build-image($(IMAGES))
build-image($(IMAGES)):
	cd $(TOP_DIR) && $(CONTAINER) build . -f $%/Dockerfile -t localhost/$%:latest


#
# Tag Images.
#
tag-images: tag-image($(IMAGES))
tag-image($(IMAGES)): require-container-registry
	cd $(TOP_DIR) && $(CONTAINER) tag localhost/$%:latest $(CONTAINER_REGISTRY)/$%:$(IMAGE_TAG)


#
# Push images.
#
push-images: push-image($(IMAGES))
push-image($(IMAGES)): require-container-registry
	cd $(TOP_DIR) && ./scripts/bin/retry.sh push $(CONTAINER_REGISTRY)/$%:$(IMAGE_TAG)


#
# Save all images.
#
save-images:
	mkdir -p "$(TOP_DIR)/build/images"
	rm -Rf "$(TOP_DIR)/build/images/all.tar"
	$(CONTAINER) save -o "$(TOP_DIR)/build/images/all.tar" $(addprefix localhost/, $(addsuffix :latest, $(IMAGES)))


#
# Load image into kind
#
kind-load: require-container-registry
	for i in $(ALL_IMAGES); do \
		kind load docker-image $(CONTAINER_REGISTRY)/$${i}:$(IMAGE_TAG); \
	done


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
# A shortcut for building and pushing the frontend only
#
frontend: host-build
	make -C console-frontend images


#
# Do a local deploy
#
# For a local deploy, we allow using the default container registry of the project
#
deploy: CONTAINER_REGISTRY ?= "ghcr.io/drogue-iot"
deploy:
	test -d deploy/helm/charts || git submodule update --init
	env TEST_CERTS_IMAGE=$(CONTAINER_REGISTRY)/test-cert-generator:$(IMAGE_TAG) ./scripts/drgadm deploy \
		-s drogueCloudCore.defaults.images.repository=$(CONTAINER_REGISTRY) \
		-s drogueCloudCore.defaults.images.tag=latest $(DEPLOY_ARGS)


#
# Check if we have a container registry set.
#
require-container-registry:
ifndef CONTAINER_REGISTRY
	$(error CONTAINER_REGISTRY is undefined)
endif


#
# Helm Lint
#
.PHONY: helm-lint
helm-lint:
	for i in core examples twin; do \
		helm dependency update deploy/helm/charts/drogue-cloud-$$i; \
		helm lint --with-subcharts deploy/helm/charts/drogue-cloud-$$i; \
	done


.PHONY: all clean pre-check check build test push images
.PHONY: require-container-registry
.PHONY: deploy gen-deploy
.PHONY: quick frontend

.PHONY: build-images tag-images push-images
.PHONY: build-image($(IMAGES)) tag-image($(IMAGES)) push-image($(IMAGES))

.PHONY: save-images
.PHONY: fix-permissions

.PHONY: container-pre-check container-check container-build container-test
.PHONY: host-pre-check host-check host-build host-test
.PHONY: cargo-pre-check cargo-check cargo-build cargo-test
