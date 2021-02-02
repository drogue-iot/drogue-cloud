CURRENT_DIR:=$(strip $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST)))))
TOP_DIR := $(CURRENT_DIR)/..

include ../Makefile

BASE=tests/certs

.PHONY: clean-certs

CA_FILES=$(BASE)/ca-cert.pem $(BASE)/ca-key.pem $(BASE)/root-cert.pem $(BASE)/root-key.pem $(BASE)/ca-cert.srl

clean-certs:
	rm -f $(CA_FILES) $(BASE)/*.pem $(BASE)/*.req $(BASE)/*.crt
	rm -f $(BASE)/device.*

create-certs: $(CA_FILES)
create-certs: $(BASE)/trusted-certs.pem
create-certs: $(BASE)/device.1.key $(BASE)/device.1.crt $(BASE)/device.1.fullchain.crt

$(BASE)/trusted-certs.pem: $(BASE)/ca-cert.pem $(BASE)/root-cert.pem
	cat $^ > $@

$(CA_FILES):
	openssl req -x509 -config "$(BASE)/ca.cnf" -nodes -newkey rsa:4096 -keyout "$(BASE)/root-key.pem" -out "$(BASE)/root-cert.pem" -days 3650 -subj "/O=Drogue IoT/OU=Cloud/CN=Application 1"

	openssl req -config "$(BASE)/ca.cnf" -reqexts intermediate_ext -nodes -newkey rsa:4096 -keyout "$(BASE)/ca-key.pem" -days 3650 -subj "/O=Drogue IoT/OU=Cloud/CN=Application 1" | \
	openssl x509 -req -extfile "$(BASE)/ca.cnf" -extensions intermediate_ext -out "$(BASE)/ca-cert.pem" -days 3650 -CA "$(BASE)/root-cert.pem" -CAkey "$(BASE)/root-key.pem" -CAcreateserial

$(BASE)/device.%.key $(BASE)/device.%.req: $(BASE)/ca.cnf
	openssl req -config "$(BASE)/ca.cnf" -nodes -newkey rsa:4096 -keyout "$(BASE)/device.$*.key" -days 3650 -subj "/O=Drogue IoT/OU=Cloud/CN=Device $*" > $(BASE)/device.$*.req

$(BASE)/device.%.crt $(BASE)/%.pem: $(BASE)/device.%.key $(BASE)/device.%.req $(BASE)/ca.cnf
	cat $(BASE)/device.$*.req | openssl x509 -req -extfile "$(BASE)/ca.cnf" -extensions "san_ext" -out "$(BASE)/device.$*.crt" -days 3650 -CA "$(BASE)/ca-cert.pem" -CAkey "$(BASE)/ca-key.pem" -CAcreateserial

$(BASE)/device.%.fullchain.crt: $(BASE)/device.%.crt $(BASE)/ca-cert.pem
	cat $^ > $@
