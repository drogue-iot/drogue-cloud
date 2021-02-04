
all: build

.PHONY: clean build

EBASE ?= endpoints
EGEN ?= $(EBASE)/generated

clean:
	rm -fR $(EGEN)

build: $(EGEN)
build: $(EGEN)/ca-bundle.pem
build: $(EGEN)/ca-key.pem $(EGEN)/ca-cert.pem
build: $(EGEN)/root-key.pem $(EGEN)/root-cert.pem

build: $(EGEN)/http-endpoint.key $(EGEN)/http-endpoint.crt $(EGEN)/http-endpoint.fullchain.crt
build: $(EGEN)/mqtt-endpoint.key $(EGEN)/mqtt-endpoint.crt $(EGEN)/mqtt-endpoint.fullchain.crt

$(EGEN):
	install -d $(EGEN)

#
# # Permissions
#
# This makefile will run in a container, and may have a different user ID then the user has on the host. This
# may result in file permission issues, which we try to fix in the Makefile automatically by making the files
# world readable. These are only test certificates, so that should be fine.
#
# Creating a directory, we can use "install" to set the file mode. For all other files, we need to use "chmod"
# afterwards. In some cases we can use the automatical variable $@. However, when a target has multiple output
# files, we only get the first and thus must explicitly specify all files.
#

# CA setup

$(EGEN)/ca-bundle.pem: $(EGEN)/ca-cert.pem $(EGEN)/root-cert.pem
	cat $^ > $@
	chmod a+r $@

$(EGEN)/root-key.pem $(EGEN)/root-cert.pem: $(EBASE)/ca.cnf
	openssl req -x509 -config "$(EBASE)/ca.cnf" -nodes -newkey rsa:4096 -keyout "$(EGEN)/root-key.pem" -out "$(EGEN)/root-cert.pem" -days 3650 -subj "/O=Drogue IoT/OU=Cloud/CN=Test Authority"
	chmod a+r $(EGEN)/root-key.pem $(EGEN)/root-cert.pem

$(EGEN)/ca-cert.req: $(EBASE)/ca.cnf
	openssl req -config "$(EBASE)/ca.cnf" -reqexts intermediate_ext -nodes -newkey rsa:4096 -keyout "$(EGEN)/ca-key.pem" -subj "/O=Drogue IoT/OU=Cloud/CN=Application" > $(EGEN)/ca-cert.req
	chmod a+r $@

$(EGEN)/ca-cert.pem: $(EBASE)/ca.cnf $(EGEN)/ca-cert.req $(EGEN)/root-cert.pem $(EGEN)/root-key.pem
	cat $(EGEN)/ca-cert.req | openssl x509 -req -extfile "$(EBASE)/ca.cnf" -extensions intermediate_ext -out "$(EGEN)/ca-cert.pem" -days 3650 -CA "$(EGEN)/root-cert.pem" -CAkey "$(EGEN)/root-key.pem" -CAcreateserial
	chmod a+r $@

# endpoint cert

$(EGEN)/%-endpoint.key $(EGEN)/%-endpoint.req: $(EBASE)/ca.cnf
	openssl req -nodes -newkey rsa:4096 -keyout "$(EGEN)/$*-endpoint.key" -subj "/O=Drogue IoT/OU=Cloud/CN=$* endpoint" -addext "subjectAltName = $(CERT_ALTNAMES)" > $(EGEN)/$*-endpoint.req
	chmod a+r $(EGEN)/$*-endpoint.key $(EGEN)/$*-endpoint.req

$(EGEN)/%-endpoint.crt: $(EBASE)/ca.cnf $(EGEN)/%-endpoint.key $(EGEN)/%-endpoint.req
	cat $(EGEN)/$*-endpoint.req | openssl x509 -req -out "$(EGEN)/$*-endpoint.crt" -extensions san_ext -extfile $(EBASE)/ca.cnf  -days 3650 -CA "$(EGEN)/ca-cert.pem" -CAkey "$(EGEN)/ca-key.pem" -CAcreateserial
	chmod a+r $@

$(EGEN)/%-endpoint.fullchain.crt: $(EGEN)/%-endpoint.crt $(EGEN)/ca-bundle.pem
	cat $^ > $@
	chmod a+r $@

