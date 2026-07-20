param(
  [Parameter(Mandatory=$true)][string]$Artifact,
  [Parameter(Mandatory=$true)][string]$CertificatePath,
  [Parameter(Mandatory=$true)][string]$TimestampUrl
)

$ErrorActionPreference = 'Stop'
if (-not $env:PARCHMINT_SIGNING_PASSWORD) {
  throw 'PARCHMINT_SIGNING_PASSWORD must be injected by the protected release environment'
}
signtool sign /fd SHA256 /td SHA256 /tr $TimestampUrl /f $CertificatePath /p $env:PARCHMINT_SIGNING_PASSWORD $Artifact
signtool verify /pa $Artifact
