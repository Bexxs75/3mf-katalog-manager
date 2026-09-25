#!/usr/bin/env python3
"""Überträgt die öffentliche GitHub-Roadmap (Projekt 1) nach Discord #roadmap.

Liest die Einträge per GraphQL und BEARBEITET zwei bestehende Webhook-
Nachrichten (DE, EN); es wird nie neu gepostet. Nur Standardbibliothek.

Umgebung:
  GH_TOKEN                 Token mit Lesezugriff auf das Projekt
  DISCORD_ROADMAP_WEBHOOK  Webhook-URL (Secret)
  DISCORD_ROADMAP_MSG_DE / _EN  IDs der beiden Nachrichten
  DRY_RUN=1                nur ausgeben, nichts an Discord senden
"""
import datetime
import json
import os
import sys
import urllib.request
import zoneinfo

OWNER, NUMBER = "Bexxs75", 1
BOARD = "https://github.com/users/Bexxs75/projects/1/views/1?groupedBy%5BcolumnId%5D=416999698"
VERSIONS = ["v0.14.0", "v0.15.0", "v0.16.0", "Später"]
ICON = {"Done": "✅", "In Progress": "🔧", "Todo": "▫️"}
PRIO_EN = {"Hoch": "high", "Mittel": "medium", "Niedrig": "low"}

QUERY = """query($o:String!,$n:Int!){user(login:$o){projectV2(number:$n){items(first:100){nodes{
  content{... on DraftIssue{title} ... on Issue{title}}
  fieldValues(first:20){nodes{... on ProjectV2ItemFieldSingleSelectValue{name field{... on ProjectV2SingleSelectField{name}}}}}
}}}}}"""


def fetch_items(token):
    body = json.dumps({"query": QUERY, "variables": {"o": OWNER, "n": NUMBER}}).encode()
    req = urllib.request.Request("https://api.github.com/graphql", body,
                                 {"Authorization": f"Bearer {token}", "Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        data = json.load(r)
    if data.get("errors"):
        sys.exit(f"GraphQL-Fehler: {data['errors']}")
    project = (data.get("data") or {}).get("user", {}).get("projectV2")
    if not project:
        sys.exit("Projekt nicht lesbar (Token ohne Projektzugriff?)")
    items = []
    for node in project["items"]["nodes"]:
        title = (node.get("content") or {}).get("title")
        fields = {v["field"]["name"]: v["name"] for v in node["fieldValues"]["nodes"] if v and v.get("field")}
        if title and fields.get("Version") in VERSIONS:
            items.append({"title": title, **fields})
    return items


def split_title(title, lang):
    de, _, en = title.partition(" / ")
    return (de if lang == "de" else (en or de)).strip()


def render(items, lang):
    today = datetime.datetime.now(zoneinfo.ZoneInfo("Europe/Berlin")).date()
    head = (f"🗺️ **Roadmap** · Stand {today:%d.%m.%Y}" if lang == "de"
            else f"🗺️ **Roadmap** · as of {today:%Y-%m-%d}")
    lines = [head]
    for version in VERSIONS:
        group = [i for i in items if i.get("Version") == version]
        if not group:
            continue
        label = version if version != "Später" else ("Später" if lang == "de" else "Later")
        lines.append(f"\n**{label}**")
        for i in group:
            text = split_title(i["title"], lang)
            if version == "Später" and i.get("Priorität"):
                prio = i["Priorität"] if lang == "de" else PRIO_EN.get(i["Priorität"], i["Priorität"])
                text += f" · _{prio}_"
            lines.append(f"{ICON.get(i.get('Status'), '▫️')} {text}")
    legend = ("✅ fertig · 🔧 in Arbeit · ▫️ geplant" if lang == "de" else "✅ done · 🔧 in progress · ▫️ planned")
    link = (f"Komplette Roadmap: <{BOARD}>" if lang == "de" else f"Full roadmap: <{BOARD}>")
    lines += ["", legend, link]
    text = "\n".join(lines)
    if len(text) > 2000:
        sys.exit(f"Nachricht ({lang}) hat {len(text)} Zeichen, Discord erlaubt 2000")
    return text


def edit(webhook, message_id, content):
    body = json.dumps({"content": content, "allowed_mentions": {"parse": []}}).encode()
    req = urllib.request.Request(f"{webhook}/messages/{message_id}", body,
                                 {"Content-Type": "application/json", "User-Agent": "3mf-roadmap-sync"}, method="PATCH")
    with urllib.request.urlopen(req, timeout=30) as r:
        r.read()


def main():
    items = fetch_items(os.environ["GH_TOKEN"])
    if not items:
        # Nie eine leere Roadmap schreiben: der Standard-Token von GitHub
        # Actions sieht die Einträge eines User-Projekts nicht.
        sys.exit("Keine Einträge gelesen - Token ohne Projektzugriff? (Secret ROADMAP_READ_TOKEN)")
    for lang in ("de", "en"):
        text = render(items, lang)
        if os.environ.get("DRY_RUN"):
            print(f"--- {lang} ({len(text)} Zeichen)\n{text}\n")
        else:
            edit(os.environ["DISCORD_ROADMAP_WEBHOOK"], os.environ[f"DISCORD_ROADMAP_MSG_{lang.upper()}"], text)
            print(f"{lang}: aktualisiert ({len(text)} Zeichen)")


if __name__ == "__main__":
    main()
