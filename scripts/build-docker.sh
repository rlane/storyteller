#!/bin/bash -eux
DOCKER_BUILDKIT=1 docker build -f Dockerfile -t storyteller .
