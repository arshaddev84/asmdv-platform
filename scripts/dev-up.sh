#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

command -v openssl >/dev/null 2>&1 || {
    printf '%s\n' "OpenSSL is required to generate local signing keys." >&2
    exit 1
}

umask 077
mkdir -p secrets

if [ ! -s secrets/jwt-private.pem ]; then
    openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:3072 -out secrets/jwt-private.pem
fi
if [ ! -s secrets/jwt-public.pem ]; then
    openssl pkey -in secrets/jwt-private.pem -pubout -out secrets/jwt-public.pem
fi
if [ ! -s secrets/dev-bootstrap-password ]; then
    openssl rand -hex 24 > secrets/dev-bootstrap-password
fi

chmod 600 secrets/jwt-private.pem secrets/dev-bootstrap-password
chmod 644 secrets/jwt-public.pem

docker compose --env-file .env.example up --build -d
printf '%s\n' "Demo sign-in: admin@northstar.example"
printf '%s\n' "Local bootstrap password is in the ignored secrets/dev-bootstrap-password file."