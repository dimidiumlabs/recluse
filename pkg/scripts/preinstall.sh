#!/bin/sh
# SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
# SPDX-License-Identifier: AGPL-3.0-or-later

set -e

PROGRAM=recluse
RECLUSE_USER=${RECLUSE_USER:-recluse}
RECLUSE_GROUP=${RECLUSE_GROUP:-${RECLUSE_USER}}

if ! getent group $RECLUSE_GROUP >/dev/null; then
  groupadd --system $RECLUSE_GROUP
fi

if ! getent passwd $RECLUSE_USER >/dev/null; then
  useradd --system --gid $RECLUSE_GROUP --no-create-home --shell /usr/sbin/nologin $RECLUSE_USER
fi
