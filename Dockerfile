# syntax=docker/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e
# SPDX-FileCopyrightText: 2026 Nikolay Govorov
# SPDX-License-Identifier: AGPL-3.0-or-later

ARG ALPINE_VERSION=3.23.5
ARG ALPINE_DIGEST=sha256:fd791d74b68913cbb027c6546007b3f0d3bc45125f797758156952bc2d6daf40
ARG DISTROLESS_DIGEST=sha256:a77defd6fedbb3392b175ba8ea3d1c22be963c1597c248c3ba987ddd80bfb512

FROM --platform=$BUILDPLATFORM docker.io/library/alpine:${ALPINE_VERSION}@${ALPINE_DIGEST} AS rootfs
RUN install -d -m 0750 /rootfs/var/lib/recluse

FROM gcr.io/distroless/cc-debian13:nonroot@${DISTROLESS_DIGEST}

ARG TARGETARCH

LABEL org.opencontainers.image.source="https://github.com/dimidiumlabs/recluse" \
      org.opencontainers.image.licenses="AGPL-3.0-or-later"

COPY --chown=root:root --chmod=0755 .container/binary-${TARGETARCH}/recluse /usr/local/bin/recluse
COPY --from=rootfs --chown=10000:10000 /rootfs/var/lib/recluse /var/lib/recluse
COPY --chown=root:root pkg/recluse.toml /etc/recluse.toml
COPY --chown=root:root LICENSE README.md /usr/share/doc/recluse/

USER 10000:10000
EXPOSE 2000
VOLUME ["/var/lib/recluse"]

ENTRYPOINT ["/usr/local/bin/recluse"]
CMD ["--config=/etc/recluse.toml"]
