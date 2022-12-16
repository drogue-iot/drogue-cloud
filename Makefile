#
# The main makefile.
#
# Be sure to read some details in the bottom of the file.
#

#
# By default, build and push artifacts and containers.
#
.PHONY: all
all: build test

CURRENT_DIR ?= $(strip $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST)))))
TOP_DIR ?= $(CURRENT_DIR)
IMAGE_TAG ?= latest
BUILDER_IMAGE ?= ghcr.io/drogue-iot/builder:0.2.5

# Control the build options (release, debug, perf)
BUILD_PROFILE ?= release

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
	authentication-service \
	coap-endpoint \
	command-endpoint \
	console-backend \
	console-frontend \
	database-migration \
	device-management-controller \
	device-management-service \
	device-state-service \
	ditto-registry-operator \
	http-endpoint \
	knative-operator \
	mqtt-endpoint \
	mqtt-integration \
	outbox-controller \
	test-cert-generator \
	topic-admin-operator \
	topic-strimzi-operator \
	ttn-operator \
	user-auth-service \
	websocket-integration \



# allow skipping the server image
ifndef SKIP_SERVER
ALL_IMAGES += server
endif


#
# Active images to build
#
IMAGES ?= $(ALL_IMAGES)


#
# Cargo build profile
#
ifeq ($(BUILD_PROFILE),)
CARGO_PROFILE=--release
else ifeq ($(BUILD_PROFILE),release)
CARGO_PROFILE=--release
else ifeq ($(BUILD_PROFILE),debug)
CARGO_PROFILE=
else ifeq ($(BUILD_PROFILE),perf)
CARGO_PROFILE=--release
CARGO_PROFILE_RELEASE_DEBUG=true
export CARGO_PROFILE_RELEASE_DEBUG
endif


#
# Restore a clean environment.
#
.PHONY: clean
clean:
	cargo clean
	rm -Rf .cargo-container-home


#
# Pre-check the code, just check checks
#
.PHONY: pre-check
pre-check: host-pre-check


#
# Check the code
#
.PHONY: check
check: host-check


#
# Build artifacts and containers.
#
.PHONY: build
ifdef SKIP_BUILD
build:
else
build: host-build
endif


#
# Run all tests.
#
.PHONY: test
test: host-test


#
# Run pre-checks on the source code
#
.PHONY: container-pre-check
container-pre-check: cargo-pre-check


#
# Run checks on the source code
#
.PHONY: container-check
container-check: cargo-check


#
# Invoked inside the container, by a call to `host-build`.
#
# If you have the same environment as the build container, you can also run this on the host, instead of `host-build`.
#
.PHONY: container-build
container-build: cargo-build
ifeq ($(MODULE),)
container-build: frontend-build
endif


#
# Invoked inside the container, by a call to `host-test`.
#
# If you have the same environment as the build container, you can also run this on the host, instead of `host-test`.
#
.PHONY: container-test
container-test: cargo-test


#
# Run pre-checks on the host, forking off into the build container.
#
.PHONY: host-pre-check
host-pre-check:
	$(CONTAINER) run $(CONTAINER_ARGS) --rm -t -v "$(TOP_DIR):/usr/src:z" "$(BUILDER_IMAGE)" make -j1 -C /usr/src/$(MODULE) container-pre-check \
		SKIP_SERVER=$(SKIP_SERVER) BUILD_PROFILE=$(BUILD_PROFILE)

#
# Run checks on the host, forking off into the build container.
#
.PHONY: host-check
host-check:
	$(CONTAINER) run $(CONTAINER_ARGS) --rm -t -v "$(TOP_DIR):/usr/src:z" "$(BUILDER_IMAGE)" make -j1 -C /usr/src/$(MODULE) container-check \
		SKIP_SERVER=$(SKIP_SERVER) BUILD_PROFILE=$(BUILD_PROFILE)


#
# Run a build on the host, forking off into the build container.
#
.PHONY: host-build
host-build:
	$(CONTAINER) run $(CONTAINER_ARGS) --rm -t -v "$(TOP_DIR):/usr/src:z" "$(BUILDER_IMAGE)" make -j1 -C /usr/src/$(MODULE) container-build \
		SKIP_SERVER=$(SKIP_SERVER) BUILD_PROFILE=$(BUILD_PROFILE)


#
# Run tests on the host, forking off into the build container.
#
.PHONY: host-test
host-test:
	if [ -z "$$($(CONTAINER) network ls --format '{{.Name}}' | grep drogue)" ]; then $(CONTAINER) network create drogue; fi
	$(CONTAINER) run $(CONTAINER_ARGS) --rm -t \
		$${RUST_LOG+-e RUST_LOG=$${RUST_LOG}} $${RUST_BACKTRACE+-e RUST_BACKTRACE=$${RUST_BACKTRACE}} \
		-v "$(TOP_DIR):/usr/src:z" $(TEST_CONTAINER_ARGS) "$(BUILDER_IMAGE)" \
		make -j1 CONTAINER=$(CONTAINER) -C /usr/src/$(MODULE) container-test



#
# Run an interactive shell inside the build container.
#
.PHONY: build-shell
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
.PHONY: cargo-pre-check
cargo-pre-check:
	cargo fmt --all -- --check


#
# Check the code
#
.PHONY: cargo-check
cargo-check: cargo-pre-check cargo-check-frontend
	cargo check $(CARGO_PROFILE)
	cargo clippy $(CARGO_PROFILE) --all-features


#
# Check the frontend project
#
.PHONY: cargo-check-frontend
cargo-check-frontend:
	cd console-frontend && cargo check $(CARGO_PROFILE)
	cd console-frontend && cargo clippy $(CARGO_PROFILE) --all-features


#
# Run the cargo build.
#
ifneq ($(MODULE),)
# build only a single package
cargo-build: CARGO_BUILD_ARGS += --package drogue-cloud-$(MODULE)
else
# build the workspace
cargo-build: CARGO_BUILD_ARGS += --workspace
ifdef SKIP_SERVER
# but exclude the server binary if requested
cargo-build: CARGO_BUILD_ARGS += --exclude drogue-cloud-server
endif
endif

.PHONY: cargo-build
cargo-build:
	@#
	@# We build everything, expect the wasm stuff. Wasm will be compiled in a separate step, and we don't need
	@# the build to compile all the dependencies, which we only use in wasm, for the standard target triple.
	@#
	cargo build $(CARGO_PROFILE) $(CARGO_BUILD_ARGS)


#
# Run the cargo tests.
#
.PHONY: cargo-test
cargo-test:
	cargo test $(CARGO_PROFILE) -- $(CARGO_TEST_OPTS)


#
# Run the frontend build.
#
.PHONY: frontend-build
frontend-build:
	cd console-frontend && npm install
	cd console-frontend && trunk build $(CARGO_PROFILE)


#
# Build images.
#
# You might want to consider doing a `build` first, but we don't enforce that.
#
.PHONY: build-images
.PHONY: build-image($(IMAGES))
ifdef SKIP_BUILD_IMAGES
build-images:
build-image($(IMAGES)):
else
build-images: build-image($(IMAGES))
build-image($(IMAGES)): | build
	cd $(TOP_DIR) && $(CONTAINER) build . -f $%/Dockerfile -t localhost/$%:latest
endif


#
# Tag Images.
#
.PHONY: tag-images
.PHONY: tag-image($(IMAGES))
ifdef SKIP_TAG_IMAGES
tag-images:
else
tag-images: tag-image($(IMAGES))
endif
tag-image($(IMAGES)): | build-image($(IMAGES))
tag-image($(IMAGES)): require-container-registry | build
	cd $(TOP_DIR) && $(CONTAINER) tag localhost/$%:latest $(CONTAINER_REGISTRY)/$%:$(IMAGE_TAG)


#
# Push images.
#
.PHONY: push-images
.PHONY: push-image($(IMAGES))
push-images: push-image($(IMAGES))
push-image($(IMAGES)): | tag-image($(IMAGES))
push-image($(IMAGES)): require-container-registry | build
	cd $(TOP_DIR) && env CONTAINER=$(CONTAINER) ./scripts/bin/retry.sh push -q $(CONTAINER_REGISTRY)/$%:$(IMAGE_TAG)


#
# Save all images.
#
.PHONY: save-images
save-images:
	mkdir -p "$(TOP_DIR)/build/images"
	rm -Rf "$(TOP_DIR)/build/images/all.tar"
	$(CONTAINER) save -o "$(TOP_DIR)/build/images/all.tar" $(addprefix localhost/, $(addsuffix :latest, $(IMAGES)))


#
# Load image into kind
#
.PHONY: kind-load
kind-load: require-container-registry
	for i in $(ALL_IMAGES); do \
		kind load docker-image $(CONTAINER_REGISTRY)/$${i}:$(IMAGE_TAG); \
	done


#
# Tag and push images.
#
.PHONY: push
push: tag-images push-images


#
# Build and push images.
#
.PHONY: images
images: build-images tag-images push-images


#
# Quick local build without tests and pushing images
#
.PHONY: quick
quick: build build-images tag-images


#
# A shortcut for building and pushing the frontend only
#
.PHONY: frontend
frontend: host-frontend
	$(MAKE) -C console-frontend images SKIP_BUILD=1


#
# Start a containerized frontend build
#
.PHONY: host-frontend
host-frontend:
	$(CONTAINER) run $(CONTAINER_ARGS) --rm -t -v "$(TOP_DIR):/usr/src:z" "$(BUILDER_IMAGE)" make -j1 -C /usr/src/$(MODULE) frontend-build \
    		SKIP_SERVER=$(SKIP_SERVER) BUILD_PROFILE=$(BUILD_PROFILE)


#
# Do a local deploy
#
# For a local deploy, we allow using the default container registry of the project
#
.PHONY: deploy
deploy: CONTAINER_REGISTRY ?= "ghcr.io/drogue-iot"
deploy:
	test -d deploy/helm/charts || git submodule update --init
	env TEST_CERTS_IMAGE=$(CONTAINER_REGISTRY)/test-cert-generator:$(IMAGE_TAG) ./scripts/drgadm deploy \
		-s drogueCloudCore.defaults.images.repository=$(CONTAINER_REGISTRY) \
		-s drogueCloudCore.defaults.images.tag=latest $(DEPLOY_ARGS)


#
# Check if we have a container registry set.
#
.PHONY: require-container-registry
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

#
# Implementation details:
#
# Container builds: The makefile is being run in two environment. On the host machine, and then inside a container, to
#   ensure that there is a sane build environment. The is covered by the same Makefile, and container targets can also
#   be called on the host machine, when necessary.
#
# Phony targets and order only: Some dependencies are declared "order only", however that doesn't work as we are using
#   mostly phony targets. This is a bug in GNU make, and turns phony order-only targets into regular phony targets. So
#   in some cases, when it is e.g. necessary to skip a build, an explicit SKIP_TARGET variable was added.
#