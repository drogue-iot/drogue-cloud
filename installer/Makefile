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
	# copy deploy folder
	cp -pr $(BASEDIR)/deploy/* $(OUTDIR)/stage/$(INSTALLER)/deploy
	# remove some unnecessary files
	rm -Rf $(OUTDIR)/stage/$(INSTALLER)/deploy/helm/.github
	rm $(OUTDIR)/stage/$(INSTALLER)/deploy/helm/.gitignore
	rm $(OUTDIR)/stage/$(INSTALLER)/deploy/helm/README.md
	# inject release version to cert script
	sed -i 's/appVersion: "latest"/appVersion: "$(VERSION)"/g' $(OUTDIR)/stage/$(INSTALLER)/deploy/helm/charts/drogue-cloud-common/Chart.yaml
	sed -i 's/appVersion: "latest"/appVersion: "$(VERSION)"/g' $(OUTDIR)/stage/$(INSTALLER)/deploy/helm/charts/drogue-cloud-core/Chart.yaml
	sed -i 's/appVersion: "latest"/appVersion: "$(VERSION)"/g' $(OUTDIR)/stage/$(INSTALLER)/deploy/helm/charts/drogue-cloud-examples/Chart.yaml
	sed -i 's/test-cert-generator:latest/test-cert-generator:$(VERSION)/g' $(OUTDIR)/stage/$(INSTALLER)/scripts/lib/common.sh
	sed -i 's/CLUSTER:=minikube/CLUSTER:=$*/' $(OUTDIR)/stage/$(INSTALLER)/scripts/lib/common.sh
	# zip it up
	@echo "::group::Create archives"
	cd build/stage/ && zip -r $(OUTDIR)/$@ $(INSTALLER)
	cd build/stage/ && tar cvzf $(OUTDIR)/$(basename $@).tar.gz $(INSTALLER)
	@echo "::endgroup::"

.PHONY: all clean prepare build
