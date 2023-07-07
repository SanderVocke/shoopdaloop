SCRIPTPATH="$( cd -- "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"

cd ${SCRIPTPATH}/../.. && podman build -f dockerfiles/ubuntu_run/ubuntu_run.Dockerfile -t sandervocke/shoopdaloop_run_ubuntu:latest . $@