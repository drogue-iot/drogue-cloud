# Changelog

## [Unreleased](https://github.com/drogue-iot/drogue-cloud/compare/v0.7.0...HEAD) (2021-11-18)

### ⚠ BREAKING CHANGE

* **mqtt:** Previously a device could deliver a message with  invalid JSON, still indicating a JSON content type. This caused issues
  in the processing of such messages throughout the system.

  A device can still deliver non-JSON payload, it only may not indicate
  that it is JSON.

* **mqtt:** This changes the topic structure of the MQTT endpoint.

### Features

* also publish container with server binary 6b2814a
* add topic operator based on Kafka admin client 007318c
* **mqtt:** Implement gateway use case for MQTT (RFC 0012) 3ff38d9
* **mqtt:** allow MQTT apps to control connect ack options 04ea0eb
* add MQTT integration to drogue-cloud-server 04e135c
* **console:** add the spy directly into the application details 0fdd5aa
* update to webpack 5 151d1e6
* add a common way to start an application, provide startup info ffe9ef6
* **frontend:** replace about image ef1cee2
* update to Rust edition 2021 2617a99
* **deploy:** Update Knative to 0.24.x c30d809
* Try using conventional commits 67e5b69

### Fixes

* prepare bringing digital twin back 4a248c6
* **console:** improve presentation of errors b506d5d
* **mqtt:** Enforce valid JSON if the content type indicates JSON a50ab23
* handle the case when two instance subscribe to the same filter 7f53181
* **mqtt:** don't unsubscribe commands on any topic 0b2e7cd
* Fix the operator queue loosing events a29a1b4
* update ntex-mqtt to fix #147 0566387
* Check for failures f581787
* Wait 15m for Helm hooks to complete and make if configurable 3f8506e
* **deploy:** Relax timeout to not run into issue with termination delay d5b1e1f
* **coap:** Enable DNS name in cert generation 3624fad
* **examples:** Use explicit listening addresses for dual stack e676909
* **installer:** Handle the case in Kind when we have multiple addresses 6cfe2e1
* base64 encode json payload for TTNv3 API 21d2c8a
* **frontend:** Add the missing topic field to the Knative example f09639b
* **auth:** Fix the crate version ff42898


## [v0.7.0](https://github.com/drogue-iot/drogue-cloud/compare/v0.6.0...v0.7.0) (2021-09-21)


## [v0.6.0](https://github.com/drogue-iot/drogue-cloud/compare/v0.5.0...v0.6.0) (2021-06-30)


## [v0.5.0](https://github.com/drogue-iot/drogue-cloud/compare/v0.4.0...v0.5.0) (2021-05-12)


## [v0.4.0](https://github.com/drogue-iot/drogue-cloud/compare/v0.3.0...v0.4.0) (2021-03-31)


## [v0.3.0](https://github.com/drogue-iot/drogue-cloud/compare/v0.1.0...v0.3.0) (2021-02-17)


## v0.1.0 (2020-09-11)


