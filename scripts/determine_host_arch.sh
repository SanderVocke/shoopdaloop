arch=$(uname -m)
if [[ "$arch" == "x86_64" ]]; then
  echo "x64"
elif [[ "$arch" == "arm64" ]]; then
  echo "arm64"
else
  echo "unknown architecture: $arch"
fi
