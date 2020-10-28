
#
# By default, build and push artifacts and containers.
#
all: build push

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
# Invoked inside the container, by a call to `host-host`.
#
# If you have the same environment as the build container, you can also run this on the host, instead of `host-build`.
#
container-build: cargo-build
container-build: webpack-build


#
# Run a build on the host, forking off into a container.
#
host-build:
	docker build containers/builder -t builder
	docker run --rm -t -v "$(CURRENT_DIR):/usr/src:z" -e MAKEFLAGS="$(MAKEFLAGS)" builder make -C /usr/src container-build


#
# Run the cargo build.
#
cargo-build:
	cargo build --release


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
build-images: require-container-registry
	docker build . -f http-endpoint/Dockerfile -t $(CONTAINER_REGISTRY)/http-endpoint:latest
	docker build . -f mqtt-endpoint/Dockerfile -t $(CONTAINER_REGISTRY)/mqtt-endpoint:latest
	docker build . -f influxdb-pusher/Dockerfile -t $(CONTAINER_REGISTRY)/influxdb-pusher:latest
	docker build . -f console-backend/Dockerfile -t $(CONTAINER_REGISTRY)/console-backend:latest
	docker build . -f console-frontend/Dockerfile -t $(CONTAINER_REGISTRY)/console-frontend:latest


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
# Alias for `push-images`.
#
push: push-images


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

.PHONY: all clean build push
.PHONY: build-images push-images
.PHONY: container-build host-build
.PHONY: cargo-build

.PHONY: require-container-registry


