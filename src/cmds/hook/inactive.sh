# shellcheck shell=bash
log_status "$(error ERROR): $(em 'Nix environment is not yet built!')" >&2
log_status "--> Use $(em __MESSAGE__) to build it." >&2
log_status "        $(m=__MESSAGE__ && em "${m//?/^}")" >&2
