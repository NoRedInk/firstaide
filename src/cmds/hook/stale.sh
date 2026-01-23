# shellcheck shell=bash
log_status "$(warning WARNING): $(em 'Nix environment is out of date!') " >&2
log_status "--> Use $(em __MESSAGE__) to rebuild it." >&2
log_status "        $(m=__MESSAGE__ && em "${m//?/^}")" >&2
log_status "$(warning WARNING): Loading $(em STALE) environment ;-(" >&2
