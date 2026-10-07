#!/usr/bin/env bash
# Regenerates logos-delivery/src/generated/ from a logos-delivery checkout using
# nim-ffi's Rust generator (the nim-ffi version is whatever that checkout pins).
#
# usage: scripts/gen-bindings.sh <path-to-logos-delivery> [rev]
set -euo pipefail

src="$(cd "${1:?path to a logos-delivery checkout}" && pwd)"
rev="${2:-$(git -C "$src" rev-parse --short=9 HEAD)}"
here="$(cd "$(dirname "$0")/.." && pwd)"
out="$here/logos-delivery/src/generated"

# --compileOnly is enough: genBindings() writes the files during macro expansion.
(cd "$src" && nim c --threads:on --mm:refc --skipParentCfg:off \
  -d:discv5_protocol_id=d5waku -d:libp2p_mix_experimental_exit_is_dest -d:libp2p_quic_support \
  -d:ffiGenBindings -d:targetLang=rust -d:ffiOutputDir=rust_bindings \
  -d:ffiSrcPath=library/liblogosdelivery.nim \
  --compileOnly library/liblogosdelivery.nim)

for f in ffi types api; do cp "$src/library/rust_bindings/src/$f.rs" "$out/$f.rs"; done

# The event API is not generated, and registering a listener needs the raw ctx pointer.
sed -i.bak 's/^    ptr: \*mut c_void,/    pub(crate) ptr: *mut c_void,/' "$out/api.rs" && rm "$out/api.rs.bak"

# build.rs emits the link directives, and only when it found the library, so a
# consumer that never calls the node (e.g. its own unit tests) still links.
sed -i.bak '/^#\[link(name = "logosdelivery")\]$/d' "$out/ffi.rs" && rm "$out/ffi.rs.bak"

# The nimble version is the only runtime signal the library exposes (see src/version.rs).
sed -n 's/^version *= *"\(.*\)"/\1/p' "$src/logos_delivery.nimble" > "$here/logos-delivery/MIN_LIBRARY_VERSION"

echo "$rev" > "$here/LOGOS_DELIVERY_REV"
