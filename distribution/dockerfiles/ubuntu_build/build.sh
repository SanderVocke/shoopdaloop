SCRIPTPATH="$( cd -- "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"

cd ${SCRIPTPATH}/../.. && podman build -f dockerfiles/ubuntu_build/ubuntu_build.Dockerfile -t sandervocke/shoopdaloop_build_ubuntu:latest . $@