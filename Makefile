all: build push

CONTAINER_REGISTRY=quay.io/use-your-own


clean:
	cargo clean

cargo-build: 
	 cargo build --release

build-docker-images:
	docker build . -f http-endpoint/Dockerfile -t $(CONTAINER_REGISTRY)/http-endpoint:latest
	docker build . -f mqtt-endpoint/Dockerfile -t $(CONTAINER_REGISTRY)/mqtt-endpoint:latest
	docker build . -f influxdb-pusher/Dockerfile -t $(CONTAINER_REGISTRY)/influxdb-pusher:latest


push:
	docker push $(CONTAINER_REGISTRY)/http-endpoint:latest
	docker push $(CONTAINER_REGISTRY)/mqtt-endpoint:latest
	docker push $(CONTAINER_REGISTRY)/influxdb-pusher:latest


docker-rust-build:
	docker build . -f http-endpoint/Staged-Dockerfile -t $(CONTAINER_REGISTRY)/http-endpoint:latest


.PHONY: all clean cargo-build build-docker-images push
