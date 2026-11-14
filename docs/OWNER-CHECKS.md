# Owner checks / Eigentümerprüfungen — 0.18.0

## English

Unchecked items are release decisions, not completed checks. No external settings were inspected or changed in this work order.

- [ ] Confirm GitHub branch protection, workflow token permissions, secret access and release ownership.
- [ ] Approve and separately review action SHA pinning / Dependabot (F3) and moving the inline workflow token to `env` (F10). This order does not change `.github/`.
- [ ] Confirm router/firewall and Tailscale ACLs: no public port or Funnel; restrict access to intended users. Review HomeBox and other services sharing the host/cookies.
- [ ] Confirm trusted writers only in catalog roots, upload/config directories; acknowledge hard-link and concurrent-writer limits (F8).
- [ ] Change generated first password; restrict access to old logs (F6). Behind a proxy, verify forwarded client addresses and direct-backend isolation (F4).
- [ ] Review post-P2 delta security evidence before accepting phase D. The historical incomplete audit is not a current full security approval.
- [ ] Complete [ABNAHME-0180.md](ABNAHME-0180.md), including the three-system native drag probe. Decide removal or hiding if the probe does not pass before release.
- [ ] Supply a catalog copy for tag analysis; approve candidate nouns/translations and merge count after backup. Arrange ES/FR review if desired.
- [ ] Decide phase E separately: image name, beta designation, timing, public announcement and budget.

## Deutsch

Offene Kästchen sind Eigentümerentscheidungen, keine bestandenen Prüfungen. Externe Einstellungen wurden in diesem Auftrag weder geprüft noch geändert.

- [ ] GitHub-Branchschutz, Workflow-Tokenrechte, Geheimniszugriff und Release-Berechtigungen prüfen.
- [ ] SHA-Pinning / Dependabot (F3) und Token-Übergabe über `env` (F10) separat freigeben und prüfen; `.github/` bleibt in diesem Auftrag unverändert.
- [ ] Router/Firewall und Tailscale-ACLs: kein öffentlicher Port/Funnel, nur berechtigte Nutzer. HomeBox und andere Dienste mit gemeinsamem Host/Cookies prüfen.
- [ ] Nur vertrauenswürdige Schreiber in Modell-, Upload- und Konfigurationsordnern; Hardlink- und Gleichzeitigkeitsschranken (F8) bestätigen.
- [ ] Erstpasswort ändern, alte Protokolle schützen (F6); Proxy-Adressen und Abschirmung des Backends prüfen (F4).
- [ ] Delta-Sicherheitsprüfung nach P2 vor Phase-D-Abnahme prüfen. Der historische unvollständige Audit ist keine aktuelle Gesamtfreigabe.
- [ ] [ABNAHME-0180.md](ABNAHME-0180.md), insbesondere Drei-System-Drag-Probe, abarbeiten. Bei fehlender Probe vor Release Entfernen oder Verbergen entscheiden.
- [ ] Katalogkopie liefern, Tag-Vorschläge/Übersetzungen und Merge-Zahl nach Sicherung bestätigen; ES/FR-Prüfung bei Bedarf organisieren.
- [ ] Phase E separat entscheiden: Image-Name, Beta-Kennzeichnung, Termin, Ankündigung und Budget.
