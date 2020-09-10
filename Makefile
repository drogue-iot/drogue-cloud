all: build push

CONTAINER_REGISTRY=quay.io/ctrontesting

clean:
	cargo clean

build:
	cargo build --release
	docker build . -f http-endpoint/Dockerfile -t $(CONTAINER_REGISTRY)/http-endpoint:latest
	docker build . -f influxdb-pusher/Dockerfile -t $(CONTAINER_REGISTRY)/influxdb-pusher:latest

push:
	docker push $(CONTAINER_REGISTRY)/http-endpoint:latest
	docker push $(CONTAINER_REGISTRY)/influxdb-pusher:latest

.PHONY: all clean build push
