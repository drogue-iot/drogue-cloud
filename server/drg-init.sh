#!/bin/bash

drg login http://localhost:8011
drg create app example-app
drg create device --application example-app device1 --spec '{"authentication":{"credentials":[{"pass":"hey-rodney"}]}}'
