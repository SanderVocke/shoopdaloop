SCRIPTPATH="$( cd -- "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
WHICH=$1

cd ${SCRIPTPATH}/.. && podman build -f dockerfiles/$1.Dockerfile -t docker.io/sandervocke/shoopdaloop_$1:latest .