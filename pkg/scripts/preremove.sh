#!/bin/sh
# Copyright (c) 2026 Nikolay Govorov
# SPDX-License-Identifier: AGPL-3.0-or-later

set -e

if [ -x "/bin/systemctl" ] && [ -d /run/systemd/system ] && [ -f /usr/lib/systemd/system/recluse.service ]; then
  /bin/systemctl stop recluse.service || true
  /bin/systemctl disable recluse.service || true
fi
