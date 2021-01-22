all: clean build


VERSION ?= latest
VARIANTS = openshift minikube kind

TOPDIR := $(strip $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST)))))
OUTDIR := $(abspath $(TOPDIR)/build)
BASEDIR := $(abspath $(TOPDIR)/../)


clean:
	rm -Rf build


prepare:
	mkdir -p build


build: prepare $(patsubst %, install-%-$(VERSION).zip, $(VARIANTS))


INSTALLER=$(basename $(notdir $@))
install-%-$(VERSION).zip: $(shell find $(TOPDIR)/../deploy -type f) $(shell find $(TOPDIR)/../hack -type f)
	mkdir -p $(OUTDIR)/stage/$(INSTALLER)/scripts
	mkdir -p $(OUTDIR)/stage/$(INSTALLER)/deploy/base
	mkdir -p $(OUTDIR)/stage/$(INSTALLER)/deploy/knative
	mkdir -p $(OUTDIR)/stage/$(INSTALLER)/deploy/$*
	mkdir -p $(OUTDIR)/stage/$(INSTALLER)/deploy/digital-twin
	cp -pr $(BASEDIR)/hack/* $(OUTDIR)/stage/$(INSTALLER)/scripts
	# FIXME: remove release related script for now, reconsider mixing scripts
	rm $(OUTDIR)/stage/$(INSTALLER)/scripts/replace-images.py
	cp -pr $(BASEDIR)/deploy/base/* $(OUTDIR)/stage/$(INSTALLER)/deploy/base
	cp -pr $(BASEDIR)/deploy/knative/* $(OUTDIR)/stage/$(INSTALLER)/deploy/knative
	cp -pr $(BASEDIR)/deploy/$*/* $(OUTDIR)/stage/$(INSTALLER)/deploy/$*
	cp -pr $(BASEDIR)/deploy/digital-twin/* $(OUTDIR)/stage/$(INSTALLER)/deploy/digital-twin
	@echo "::group::Process YAML and zip"
	cd build/stage/ && $(BASEDIR)/hack/replace-images.py $(VERSION) IfNotPresent $(OUTDIR)/stage/$(INSTALLER)/deploy/ && zip -r $(OUTDIR)/$@ $(INSTALLER)
	@echo "::endgroup::"


.PHONY: all clean prepare build
