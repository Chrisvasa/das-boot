#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
requested_port="${1:-}"

if [[ -n "$requested_port" ]]; then
  serial_port="$requested_port"
elif compgen -G '/dev/serial/by-id/*' >/dev/null; then
  serial_port=""
  for candidate in /dev/serial/by-id/*; do
    resolved="$(readlink -f "$candidate")"
    if [[ "$(basename "$resolved")" == ttyACM* ]]; then
      serial_port="$candidate"
      break
    fi
  done

  if [[ -z "$serial_port" && -e /dev/ttyACM0 ]]; then
    serial_port="/dev/ttyACM0"
  fi

elif [[ -e /dev/ttyACM0 ]]; then
  serial_port="/dev/ttyACM0"
else
  serial_port=""
fi

if [[ -z "$serial_port" ]]; then
  echo "Ingen STM32 USB CDC-port hittades." >&2
  echo "Anslut kortet och kontrollera med: ls -l /dev/ttyACM* /dev/serial/by-id/" >&2
  exit 1
fi

if [[ ! -r "$serial_port" || ! -w "$serial_port" ]]; then
  echo "Saknar läs-/skrivrättighet till $serial_port." >&2
  echo "Kontrollera portens grupp med: ls -l $serial_port" >&2
  exit 1
fi

echo "Startar das-boot API mot $serial_port"
export DOTNET_ENVIRONMENT=Development
export Stm32__PortName="$serial_port"
exec dotnet run --project "$project_dir/DasBoot.Api.csproj"
