all: clean build


VERSION ?= latest
VARIANTS = openshift minikube kind

TOPDIR := $(strip $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST)))))
OUTDIR := $(abspath $(TOPDIR)/build)


clean:
	rm -Rf build


prepare:
	mkdir -p build


build: prepare $(patsubst %, install-%-$(VERSION).zip, $(VARIANTS))


INSTALLER=$(basename $(notdir $@))
install-%-$(VERSION).zip: $(shell find $(TOPDIR)/../deploy -type f -name '*.yaml' -not -path "*/deploy/build/*")
	mkdir -p build/stage/$(INSTALLER)/base
	mkdir -p build/stage/$(INSTALLER)/$*
	cp -pr $(TOPDIR)/../deploy/base/* build/stage/$(INSTALLER)/base
	cp -pr $(TOPDIR)/../deploy/$*/* build/stage/$(INSTALLER)/$*
	cd build/stage/ && $(TOPDIR)/../hack/inject-release-images.py $(VERSION) IfNotPresent $(abspath $(OUTDIR)/stage/$(INSTALLER)/) &&  zip -r $(OUTDIR)/$@ $(INSTALLER)


.PHONY: all clean prepare build
