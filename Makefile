all: build push

clean:
	cargo clean

build:
	cargo build --release
	docker build . -f http-endpoint/Dockerfile -t quay.io/ctrontesting/http-endpoint:latest

push:
	docker push quay.io/ctrontesting/http-endpoint:latest

.PHONY: all clean build push