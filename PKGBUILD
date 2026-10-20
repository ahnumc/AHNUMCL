# Maintainer: AHNUMC

pkgname=ahnumcl-bin
pkgdesc='A Minecraft launcher based on SJMCL'
pkgver=0.0.0
_github_pkgver=0.0.0
pkgrel=1
arch=('x86_64' 'aarch64')
license=(GPL-3.0,custom:LICENSE.EXTRA)
url='https://github.com/ahnumc/AHNUMCL'
_baseurl="${url}/releases/download/v${_github_pkgver}"
_source="AHNUMCL_${_github_pkgver}_linux_${CARCH}.deb"

sha256sums=('SKIP')
sha256sums_x86_64=('SKIP')
sha256sums_aarch64=('SKIP')

source=('LICENSE.EXTRA')
source_x86_64=("${_baseurl}/${_source}")
source_aarch64=("${_baseurl}/${_source}")
depends=('cairo' 'desktop-file-utils' 'gdk-pixbuf2' 'glib2' 'gtk3' 'hicolor-icon-theme' 'libsoup3' 'pango' 'webkit2gtk-4.1')
options=('!strip' '!emptydirs')
provides=('ahnumcl')
conflicts=('ahnumcl')

package() {
  bsdtar -xf data.tar.gz -C "${pkgdir}"
  chmod +x ${pkgdir}/usr/bin/AHNUMCL
  install -Dm 644 "${srcdir}/LICENSE.EXTRA" "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE.EXTRA"
}
