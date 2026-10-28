#!/usr/bin/env bash
set -euo pipefail

if [[ $(uname -s) != Darwin || $(uname -m) != x86_64 ]]; then
  echo 'Catalina dependencies must be built on an Intel macOS host.' >&2
  exit 1
fi

mkdir -p "$1/lib" "$1/licenses"
destination=$(cd "$1" && pwd)
build_dir=$(mktemp -d)
trap 'rm -rf "$build_dir"' EXIT
export MACOSX_DEPLOYMENT_TARGET=10.15
cd "$build_dir"

curl -L --fail --retry 3 -o openssl.tar.gz https://github.com/openssl/openssl/releases/download/openssl-3.5.9/openssl-3.5.9.tar.gz
echo '603f5602e2eef00d77fbd429d34dcd5822bb301757a1bc9cdb24c670f1eb859a  openssl.tar.gz' | shasum -a 256 --check
tar -xzf openssl.tar.gz
cd openssl-3.5.9
./Configure darwin64-x86_64-cc shared no-tests no-module \
  --prefix="$destination/openssl" --openssldir=/etc/ssl
make -j3 build_libs
cp libcrypto.3.dylib "$destination/lib/"
cp LICENSE.txt "$destination/licenses/OpenSSL-LICENSE.txt"
cd "$build_dir"

curl -L --fail --retry 3 -o xz.tar.gz https://github.com/tukaani-project/xz/releases/download/v5.8.4/xz-5.8.4.tar.gz
echo '0014c7886930454fe8bd4228665b51af55eeae560ea135c9c4cd33f55b2591d9  xz.tar.gz' | shasum -a 256 --check
tar -xzf xz.tar.gz
cd xz-5.8.4
./configure --prefix="$destination/xz" --disable-static --enable-shared \
  --disable-nls --disable-doc --disable-xz --disable-xzdec --disable-lzmadec \
  --disable-lzmainfo --disable-lzma-links --disable-scripts
make -j3
make install
cp -L "$destination/xz/lib/liblzma.5.dylib" "$destination/lib/"
cp COPYING "$destination/licenses/XZ-COPYING"
cp COPYING.0BSD "$destination/licenses/XZ-COPYING.0BSD"

for library in libcrypto.3.dylib liblzma.5.dylib; do
  install_name_tool -id "@rpath/$library" "$destination/lib/$library"
  codesign --force --sign - "$destination/lib/$library"
done
