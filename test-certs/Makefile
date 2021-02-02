
all: build

.PHONY: clean build

EBASE=endpoints
EGEN=$(EBASE)/generated

clean:
	rm -fR $(EGEN)

build: $(EGEN)
build: $(EGEN)/ca-bundle.pem
build: $(EGEN)/ca-key.pem $(EGEN)/ca-cert.pem
build: $(EGEN)/root-key.pem $(EGEN)/root-cert.pem

build: $(EGEN)/http-endpoint.key $(EGEN)/http-endpoint.crt $(EGEN)/http-endpoint.fullchain.crt

$(EGEN):
	mkdir -p $(EGEN)

# CA setup

$(EGEN)/ca-bundle.pem: $(EGEN)/ca-cert.pem $(EGEN)/root-cert.pem
	cat $^ > $@

$(EGEN)/root-key.pem $(EGEN)/root-cert.pem: $(EBASE)/ca.cnf
	openssl req -x509 -config "$(EBASE)/ca.cnf" -nodes -newkey rsa:4096 -keyout "$(EGEN)/root-key.pem" -out "$(EGEN)/root-cert.pem" -days 3650 -subj "/O=Drogue IoT/OU=Cloud/CN=Test Authority"

$(EGEN)/ca-cert.req: $(EBASE)/ca.cnf
	openssl req -config "$(EBASE)/ca.cnf" -reqexts intermediate_ext -nodes -newkey rsa:4096 -keyout "$(EGEN)/ca-key.pem" -days 3650 -subj "/O=Drogue IoT/OU=Cloud/CN=Application 1" > $(EGEN)/ca-cert.req

$(EGEN)/ca-cert.pem: $(EBASE)/ca.cnf $(EGEN)/ca-cert.req $(EGEN)/root-cert.pem $(EGEN)/root-key.pem
	cat $(EGEN)/ca-cert.req | openssl x509 -req -extfile "$(EBASE)/ca.cnf" -extensions intermediate_ext -out "$(EGEN)/ca-cert.pem" -days 3650 -CA "$(EGEN)/root-cert.pem" -CAkey "$(EGEN)/root-key.pem" -CAcreateserial

# HTTP endpoint

$(EGEN)/http-endpoint.key $(EGEN)/http-endpoint.req: $(EBASE)/ca.cnf
	openssl req -config "$(EBASE)/ca.cnf" -nodes -newkey rsa:4096 -keyout "$(EGEN)/http-endpoint.key" -days 3650 -subj "/O=Drogue IoT/OU=Cloud/CN=HTTP Endpoint" > $(EGEN)/http-endpoint.req

$(EGEN)/http-endpoint.crt: $(EBASE)/ca.cnf $(EGEN)/http-endpoint.key $(EGEN)/http-endpoint.req
	cat $(EGEN)/http-endpoint.req | openssl x509 -req -extfile "$(EBASE)/ca.cnf" -extensions "san_ext" -out "$(EGEN)/http-endpoint.crt" -days 3650 -CA "$(EGEN)/ca-cert.pem" -CAkey "$(EGEN)/ca-key.pem" -CAcreateserial

$(EGEN)/http-endpoint.fullchain.crt: $(EGEN)/http-endpoint.crt $(EGEN)/ca-cert.pem
	cat $^ > $@
