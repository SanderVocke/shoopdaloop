#!/bin/bash

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
source $SCRIPT_DIR/set_variables.sh

# output the template
cat  << EOF
# Maintainer: Sander Vocke <sandervocke@gmail.com>
_base=shoopdaloop
pkgname=\${_base}-git
provides=(\${_base})
pkgver="git"
pkgrel=1
pkgdesc="${DESCRIPTION}"
arch=(any)
url="https://github.com/SanderVocke/\${_base}"
license=('LGPL')
depends=(${DEPENDS})
makedepends=(git ${MAKEDEPENDS})
checkdepends=(${CHECKDEPENDS})
source=(shoopdaloop::git+\${url}#branch=restructure)
sha512sums=('SKIP')

pkgver() {
  cd "\${srcdir}/\${_base}"
  printf "r%s.%s" "\$(git rev-list --count HEAD)" "\$(git rev-parse --short HEAD)"
}

build() {
  cd \${srcdir}/\${_base}
  git submodule init
  git submodule update
  python -m build --wheel --skip-dependency-check --no-isolation
}

check() {
  cd \${srcdir}/\${_base}
  python -m venv --system-site-packages test-env
  test-env/bin/python -m installer dist/*.whl
  #TODO test-env/bin/python -m pytest
}

package() {
  cd \${srcdir}/\${_base}
  PYTHONPYCACHEPREFIX="\${PWD}/.cache/cpython/" python -m installer -d \${pkgdir} dist/*.whl
  install -Dm 644 LICENSE -t "\${pkgdir}/usr/share/licenses/\${_base}"
  install -Dm 644 README.md -t "\${pkgdir}/usr/share/doc/\${_base}"
}
EOF