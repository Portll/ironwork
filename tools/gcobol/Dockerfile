# GCC's COBOL front end. libgcobol supports aarch64 and x86-64 Linux; on an Apple silicon Mac this
# runs natively under Docker Desktop's arm64 Linux.
#   docker build -t gcobol:16 tools/gcobol
FROM debian:testing@sha256:16faa8d1cd99fcb2d30eebe90454e26b20499055fa7094e9b55d32d1a7666f08
RUN apt-get update && apt-get install -y --no-install-recommends gcobol gcc libc6-dev && rm -rf /var/lib/apt/lists/*
