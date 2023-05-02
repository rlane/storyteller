FROM rust:1.68.0-slim-buster as builder
WORKDIR /usr/src/app
RUN apt-get update
RUN apt-get install -y protobuf-compiler
COPY . .
RUN \
  --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=cache,target=/usr/local/cargo/git \
  --mount=type=cache,target=/usr/src/app/target,id=storyteller_target,sharing=locked \
  cargo install --locked --path .

FROM rust:1.68.0-slim-buster
RUN useradd -m app
USER app:1000
WORKDIR /home/app
COPY --from=builder /usr/local/cargo/bin/storyteller /usr/local/bin/storyteller
ENV PORT 8080
ENV RUST_LOG none,storyteller=info
CMD ["storyteller"]
