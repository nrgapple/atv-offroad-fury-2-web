# fury-server in a container: the same static binary CI builds and deploys
# (the "fury-server" artifact of .github/workflows/game-server.yml, or
# `cargo build --release -p atv-server --bin fury-server --target x86_64-unknown-linux-musl`).
#
#   docker build -f deploy/Dockerfile -t fury-server path/to/dir-with-binary
#   docker run -p 8789:8789 -e FURY_LISTEN=0.0.0.0:8789 fury-server
#
# Settings are the environment variables in deploy/env.example. Mount a volume
# at /var/lib/fury-server to keep saved rooms and ACME certificates.
# distroless has no shell: make the state directory, owned by nonroot (65532), here.
FROM busybox:1.36.1 AS state
RUN mkdir -p /out/var/lib/fury-server && chown -R 65532:65532 /out/var/lib/fury-server

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=state /out/ /
COPY --chmod=755 fury-server /usr/local/bin/fury-server
ENV FURY_LISTEN=0.0.0.0:8789 \
    FURY_TLS=off \
    FURY_STATE_DIR=/var/lib/fury-server/state \
    FURY_ACME_CACHE=/var/lib/fury-server/acme \
    RUST_LOG=info
VOLUME /var/lib/fury-server
EXPOSE 8789 443
USER nonroot:nonroot
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/fury-server"]
