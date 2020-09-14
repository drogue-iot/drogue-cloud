## Testing

### Create self signed cert

    openssl req -x509 -nodes -subj '/CN=localhost' -newkey rsa:4096 -keyout examples/key8.pem -out examples/cert.pem -days 365 -keyform PEM
    openssl rsa -in examples/key8.pem -out examples/key.pem
