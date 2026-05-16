#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/../.."

rm -rf contents public dist

mkdir -p contents/articles contents/users public
cp -R test_config/test_data/e2e_test/contents/articles/. contents/articles/
cp -R test_config/test_data/e2e_test/contents/users/. contents/users/
cp -R test_config/test_data/e2e_test/public/. public/

PUBLIC_KEY_FILE="test_config/public-key-for-test.pem" SITE_URL="https://blog.test" pnpm run build
