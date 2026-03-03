#!/usr/bin/env bash
set -euo pipefail

BIN="${1:-rollup_bc}"
FUNC_RE="${2:-validate_chain_id}"

if [[ ! -f "$BIN" ]]; then
  echo "binary not found: $BIN" >&2
  exit 1
fi

MACHINE="$(readelf -h "$BIN" 2>/dev/null | awk -F: '/Machine:/{gsub(/^[[:space:]]+/, "", $2); print $2; exit}')"
case "$MACHINE" in
  *AArch64*) ARCH="aarch64" ;;
  *X86-64*|*x86-64*) ARCH="x86_64" ;;
  *) ARCH="other" ;;
esac

nm -an "$BIN" | grep "$FUNC_RE" | awk '{print $3}' |
while IFS= read -r sym; do
  [[ -z "$sym" ]] && continue
  echo "== $sym =="

  if [[ "$ARCH" == "x86_64" ]]; then
    hexes="$(
      objdump -d -M intel --disassemble="$sym" "$BIN" \
        | grep -E '^[[:space:]]*[0-9a-f]+:.*[[:space:]]cmp[[:space:]].*0x[0-9a-f]+' \
        | sed -nE 's/.*,(0x[0-9a-f]+)[[:space:]]*$/\1/p' \
        | sort -u || true
    )"
  elif [[ "$ARCH" == "aarch64" ]]; then
    hexes="$(
      objdump -d -M no-aliases --disassemble="$sym" "$BIN" \
        | awk '
          function reg_id(r, t, m) {
            t = r
            gsub(/,/, "", t)
            if (match(t, /^[wx]([0-9]+)$/, m)) return m[1]
            return ""
          }
          function parse_imm(s, t) {
            t = s
            gsub(/,/, "", t)
            sub(/^#/, "", t)
            if (t !~ /^0x[0-9a-fA-F]+$/) return ""
            return strtonum(t)
          }
          function set_halfword(val, imm, shift, base, chunk) {
            base = 2 ^ shift
            chunk = int(val / base) % 65536
            return val - (chunk * base) + (imm * base)
          }
          {
            addr = ""
            op = ""
            args = ""

            if (match($0, /^[[:space:]]*([0-9a-f]+):/, a)) addr = a[1]
            if (match($0, /[[:space:]]([[:alpha:]][[:alnum:].]*)[[:space:]]+([^[:space:]].*)$/, m)) {
              op = m[1]
              args = m[2]
            } else {
              next
            }

            if (op == "movz" || op == "mov") {
              if (match(args, /^([wx][0-9]+),[[:space:]]*(#[^,[:space:]]+)(,[[:space:]]*lsl[[:space:]]*#([0-9]+))?$/, z)) {
                r = reg_id(z[1])
                imm = parse_imm(z[2])
                sh = (z[4] == "" ? 0 : z[4] + 0)
                if (r != "" && imm != "") {
                  val[r] = imm * (2 ^ sh)
                  have[r] = 1
                }
              }
            } else if (op == "movk") {
              if (match(args, /^([wx][0-9]+),[[:space:]]*(#[^,[:space:]]+)(,[[:space:]]*lsl[[:space:]]*#([0-9]+))?$/, k)) {
                r = reg_id(k[1])
                imm = parse_imm(k[2])
                sh = (k[4] == "" ? 0 : k[4] + 0)
                if (r != "" && imm != "") {
                  if (!(r in have)) val[r] = 0
                  val[r] = set_halfword(val[r], imm, sh)
                  have[r] = 1
                }
              }
            } else if (op == "cmp") {
              if (match(args, /^([wx][0-9]+),[[:space:]]*(#[^,[:space:]]+)$/, ci)) {
                imm = parse_imm(ci[2])
                if (imm != "") printf("0x%x\n", imm)
              } else if (match(args, /^([wx][0-9]+),[[:space:]]*([wx][0-9]+)$/, cr)) {
                r1 = reg_id(cr[1])
                r2 = reg_id(cr[2])
                if (r1 != "" && (r1 in have)) printf("0x%x\n", val[r1])
                if (r2 != "" && (r2 in have)) printf("0x%x\n", val[r2])
              }
            }
          }
        ' | sort -u || true
    )"
  else
    echo "  unsupported machine type: ${MACHINE:-unknown}"
    continue
  fi

  if [[ -z "$hexes" ]]; then
    echo "  (no cmp-related constants found)"
    continue
  fi

  while IFS= read -r h; do
    [[ -z "$h" ]] && continue
    printf "  %s (%d)\n" "$h" "$((h))"
  done <<< "$hexes"
done
