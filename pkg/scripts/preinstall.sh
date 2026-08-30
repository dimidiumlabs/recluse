#!/bin/sh
# SPDX-FileCopyrightText: 2026 Nikolay Govorov
# SPDX-License-Identifier: AGPL-3.0-or-later

set -e

TESOR_USER=${TESOR_USER:-tesor}
TESOR_GROUP=${TESOR_GROUP:-${TESOR_USER}}

if ! getent group "$TESOR_GROUP" >/dev/null; then
  groupadd --system "$TESOR_GROUP"
fi

if ! getent passwd "$TESOR_USER" >/dev/null; then
  useradd --system --gid "$TESOR_GROUP" --no-create-home --shell /usr/sbin/nologin "$TESOR_USER"
fi
