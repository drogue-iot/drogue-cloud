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


build: prepare $(patsubst %, drogue-install-%-$(VERSION).zip, $(VARIANTS))


INSTALLER=$(basename $(notdir $@))
drogue-install-%-$(VERSION).zip: $(shell find $(TOPDIR)/../deploy -type f) $(shell find $(TOPDIR)/../scripts -type f)
	mkdir -p $(OUTDIR)/stage/$(INSTALLER)/scripts
	mkdir -p $(OUTDIR)/stage/$(INSTALLER)/deploy
	# copy scripts folder
	cp -pr $(BASEDIR)/scripts/* $(OUTDIR)/stage/$(INSTALLER)/scripts
	# inject release version to cert script
	sed -i 's/test-cert-generator:latest/test-cert-generator:$(VERSION)/g' $(OUTDIR)/stage/$(INSTALLER)/scripts/lib/common.sh
	sed -i 's/CLUSTER:=minikube/CLUSTER:=$*/' $(OUTDIR)/stage/$(INSTALLER)/scripts/lib/common.sh
	# copy deploy folder
	cp -pr $(BASEDIR)/deploy/* $(OUTDIR)/stage/$(INSTALLER)/deploy
	@echo "::group::Create archives"
	cd build/stage/ && zip -r $(OUTDIR)/$@ $(INSTALLER)
	cd build/stage/ && tar cvzf $(OUTDIR)/$(basename $@).tar.gz $(INSTALLER)
	@echo "::endgroup::"

.PHONY: all clean prepare build
