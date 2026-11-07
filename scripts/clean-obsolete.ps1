# Borra los archivos que la actualización 04.2 retiró (un ZIP de «solo nuevos/modificados» no puede borrar).
# Ejecutar desde la RAÍZ del proyecto:  powershell -ExecutionPolicy Bypass -File scripts\clean-obsolete.ps1
$ErrorActionPreference = "Stop"
if (-not (Test-Path "package.json")) { Write-Error "Ejecútalo desde la raíz del proyecto (donde está package.json)."; exit 1 }
$removed = 0
Get-Content "scripts\obsolete-04.2.txt" | Where-Object { $_.Trim() } | ForEach-Object {
  $p = $_.Trim() -replace "/", "\"
  if (Test-Path $p) { Remove-Item $p -Force; Write-Host "borrado  $p"; $removed++ }
}
Write-Host "Listo: $removed archivo(s) obsoleto(s) borrado(s)."
