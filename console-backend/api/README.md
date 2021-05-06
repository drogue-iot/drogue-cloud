# API definition

This directory contains the API definition. It must be a single file named `index.yaml`
and will be served (pre-processed) by the console backend.

## Editing

If you want to edit this file, you can get a live preview by running the following
from the root of the repository:

    podman run -p 8083:8080 -e SWAGGER_JSON=/drogue/index.yaml -v "$PWD/console-backend/api:/drogue:z" docker.io/swaggerapi/swagger-ui:latest

And then, navigate to http://localhost:8083
