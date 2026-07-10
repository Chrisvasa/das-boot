# das-boot dashboard

Vanilla HTML, CSS och JavaScript. Inga npm-paket, bundlers eller frontendramverk.
Filerna serveras direkt av ASP.NET Core från `wwwroot`.

## Videoström

Ändra `streamMode` och `streamUrl` i `config.js`:

```js
streamMode: "mjpeg",
streamUrl: "/camera/stream"
```

- `mjpeg`: använder en vanlig `<img>` och passar en MJPEG-endpoint.
- `video`: använder `<video playsinline>` för ett format webbläsaren kan spela direkt.

Det är bäst att reverse-proxya kameraprocessen under samma host, exempelvis
`/camera/stream`. Då undviks CORS, mixed-content och separata certifikat.

## Servokanaler

Kanaler, namn och pulse width-gränser ändras i `config.js`. Standard är:

- kanal 0: roder
- kanal 1: dykroder
- kanal 2: kameratilt

Frontend begränsar utskicken per kanal och behåller bara det senaste reglagevärdet.

## Pilotlås

Pilotlåset är en lease i API-processens minne:

- endast en klient får en giltig token,
- token krävs i `X-Pilot-Token` för servo-endpointen,
- webbläsaren skickar heartbeat,
- låset släpps automatiskt efter 15 sekunder utan heartbeat.

Detta löser kontrollarbitrering men är inte användarautentisering. Lägg till riktig
autentisering och TLS om nätverket inte är helt betrott.

## Telemetri

Nuvarande firmware beskriver ännu inte payloadformatet för batteri, djup och temperatur.
Rådata visas i systempanelen. Lägg den slutliga avkodningen i `decodeTelemetry()` i
`app.js`, eller ännu hellre i ett typat API-svar på serversidan.

## Säkerhet

Pilotlåset ersätter inte ett firmware-watchdog. STM32 bör själv försätta motorer och
servon i ett säkert läge när giltiga kontrollkommandon upphör under en bestämd period.
