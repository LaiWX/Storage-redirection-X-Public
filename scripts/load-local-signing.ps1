$privateConfigPath = Join-Path $env:USERPROFILE '.android/storage-redirect-x-signing/signing.json'
$privateConfig = Get-Content -LiteralPath $privateConfigPath -Raw | ConvertFrom-Json
if (-not (Test-Path -LiteralPath $privateConfig.keystorePath)) { throw 'Existing signing keystore not found' }
$securePassword = ConvertTo-SecureString -String $privateConfig.passwordDpapi
$passwordPointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($securePassword)
try {
    $env:SRX_APP_SIGNING_STORE_FILE = $privateConfig.keystorePath
    $env:SRX_APP_SIGNING_KEY_ALIAS = $privateConfig.keyAlias
    $env:SRX_APP_SIGNING_STORE_PASSWORD = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($passwordPointer)
    $env:SRX_APP_SIGNING_KEY_PASSWORD = $env:SRX_APP_SIGNING_STORE_PASSWORD
} finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($passwordPointer)
    $securePassword.Dispose()
    Remove-Variable privateConfig, passwordPointer -ErrorAction SilentlyContinue
}
