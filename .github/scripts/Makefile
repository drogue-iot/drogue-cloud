include ../../Makefile

#
# Build images in github CI environment.
#
# This caches image layers in a specific path, so we can use GitHub actions cache to speed up the build.
#
build-images-ci: build-image-ci($(IMAGES))
build-image-ci($(IMAGES)):
	cd $(TOP_DIR) && docker buildx build \
							--cache-from "type=local,src=/tmp/.buildx-cache" \
							--cache-to "type=local,dest=/tmp/.buildx-cache" \
							--load \
							. -f $%/Dockerfile -t $%:$(IMAGE_TAG)


#
# Load local images into kind.
#
kind-load-images: kind-load-image($(IMAGES))
kind-load-image($(IMAGES)): require-container-registry
	kind load docker-image $(CONTAINER_REGISTRY)/$%:$(IMAGE_TAG)
