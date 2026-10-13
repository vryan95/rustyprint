# The rustyprint HTTP service, from static Linux binaries built beforehand. For one platform:
#
#   cargo zigbuild --release -p rustyprint-server --target x86_64-unknown-linux-musl
#   mkdir -p docker/bin/amd64
#   cp target/x86_64-unknown-linux-musl/release/rustyprint-server docker/bin/amd64/
#   docker build -f docker/server.Dockerfile -t rustyprint-server docker
#
# The release workflow builds both linux/amd64 and linux/arm64 this way. The image holds
# nothing but the program: no shell, and it runs as an unprivileged user.
FROM gcr.io/distroless/static-debian12:nonroot
ARG TARGETARCH
COPY bin/${TARGETARCH}/rustyprint-server /usr/local/bin/rustyprint-server
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/rustyprint-server"]
