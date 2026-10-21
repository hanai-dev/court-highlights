# syntax=docker/dockerfile:1.7
# Online service image (HOST-container-*): built from the reviewed commit, base images pinned by digest, runs as a
# non-root user on a read-only root filesystem (run with --read-only), and declares its own health check.
#   docker build -f docker/cloud.Dockerfile -t ch-cloud:<commit> .

FROM rust:1.99.0-slim-trixie@sha256:2752b332db73fdbb7dc576f06c82ed1f312005784ef913d7e04a28f5f55dc581 AS build
WORKDIR /src
# The shared rules crate takes its edition and versions from the root workspace, whose manifest names every member,
# so the crates come in whole; only the online service is compiled.
COPY Cargo.toml ./
COPY crates crates
RUN cargo build --release --locked --manifest-path crates/cloud/Cargo.toml

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:83b9b50586253821cf63ffcec87d3d94265a093ad30da232039af962dd8f4edf
COPY --from=build /src/crates/cloud/target/release/ch-cloud /usr/local/bin/ch-cloud
USER nonroot:nonroot
EXPOSE 8080
# The platform restarts the container when this fails (HOST-container-health).
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 CMD ["/usr/local/bin/ch-cloud", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/ch-cloud"]
