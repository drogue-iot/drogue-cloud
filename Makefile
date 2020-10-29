
#
# By default, build and push artifacts and containers.
#

all: build test push


CURRENT_DIR:=$(strip $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST)))))


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
build: build-images

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
container-build: webpack-build


#
# Invoked inside the container, by a call to `host-test`.
#
# If you have the same environment as the build container, you can also run this on the host, instead of `host-test`.
#
container-test: cargo-test


#
# Create the builder image
#
build-builder:
	docker build containers/builder -t builder


#
# Run a build on the host, forking off into the build container.
#
host-build: build-builder
	docker run --rm -t -v "$(CURRENT_DIR):/usr/src:z" -e MAKEFLAGS="$(MAKEFLAGS)" builder make -C /usr/src container-build


#
# Run tests on the host, forking off into the build container.
#
host-test: build-builder
	docker run --rm -t -v "$(CURRENT_DIR):/usr/src:z" -e MAKEFLAGS="$(MAKEFLAGS)" builder make -C /usr/src container-test


#
# Run the cargo build.
#
cargo-build:
	@#
	@# We build everything, expect the wasm stuff
	@#
	cargo build --release --workspace --exclude console-frontend


#
# Run the cargo tests.
#
cargo-test:
	cargo test --release


#
# Run the webpack build.
#
webpack-build:
	cd console-frontend && npm install
	cd console-frontend && npm run build


#
# Build images.
#
# You might want to consider doing a `build` first.
#
build-images:
	docker build . -f http-endpoint/Dockerfile -t http-endpoint:latest
	docker build . -f mqtt-endpoint/Dockerfile -t mqtt-endpoint:latest
	docker build . -f influxdb-pusher/Dockerfile -t influxdb-pusher:latest
	docker build . -f console-backend/Dockerfile -t console-backend:latest
	docker build . -f console-frontend/Dockerfile -t console-frontend:latest


#
# Tag Images.
#
tag-images: require-container-registry
	docker tag http-endpoint:latest $(CONTAINER_REGISTRY)/http-endpoint:latest
	docker tag mqtt-endpoint:latest $(CONTAINER_REGISTRY)/mqtt-endpoint:latest
	docker tag influxdb-pusher:latest $(CONTAINER_REGISTRY)/influxdb-pusher:latest
	docker tag console-backend:latest $(CONTAINER_REGISTRY)/console-backend:latest
	docker tag console-frontend:latest $(CONTAINER_REGISTRY)/console-frontend:latest


#
# Push images.
#
push-images: require-container-registry
	docker push $(CONTAINER_REGISTRY)/http-endpoint:latest
	docker push $(CONTAINER_REGISTRY)/mqtt-endpoint:latest
	docker push $(CONTAINER_REGISTRY)/influxdb-pusher:latest
	docker push $(CONTAINER_REGISTRY)/console-backend:latest
	docker push $(CONTAINER_REGISTRY)/console-frontend:latest


#
# Tag and push images.
#
push: tag-images push-images


#
# Build and push images.
#
images: build-images push-images


#
# Check if we have a container registry set.
#
require-container-registry:
ifndef CONTAINER_REGISTRY
	$(error CONTAINER_REGISTRY is undefined)
endif

.PHONY: all clean build test push
.PHONY: build-builder
.PHONY: build-images tag-images push-images images
.PHONY: container-build container-test
.PHONY: host-build host-test
.PHONY: cargo-build cargo-test

.PHONY: require-container-registry

