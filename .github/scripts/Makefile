include ../../Makefile

#
# Load local images into kind.
#
kind-load-images: kind-load-image($(IMAGES))
kind-load-image($(IMAGES)): require-container-registry
	kind load docker-image $(CONTAINER_REGISTRY)/$%:$(IMAGE_TAG)

