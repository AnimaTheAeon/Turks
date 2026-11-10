#!/bin/sh
# Forwards requests from the Turks window to the Turks service.
exec /var/packages/Turks/target/bin/turks cgi
