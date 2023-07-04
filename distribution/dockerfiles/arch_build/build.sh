SCRIPTPATH="$( cd -- "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"

cd ${SCRIPTPATH}/../.. && podman build -f dockerfiles/arch_build/arch_build.Dockerfile -t sandervocke/shoopdaloop_build_arch:latest . $@