all: build push

CONTAINER_REGISTRY=quay.io/ctrontesting

clean:
	cargo clean

build:
	cargo build --release
	docker build . -f http-endpoint/Dockerfile -t $(CONTAINER_REGISTRY)/http-endpoint:latest

push:
	docker push $(CONTAINER_REGISTRY)/http-endpoint:latest

.PHONY: all clean build push
