#!/usr/bin/env python3
"""Überträgt die öffentliche GitHub-Roadmap (Projekt 1) nach Discord #roadmap.

Liest die Einträge per GraphQL und BEARBEITET vier bestehende Webhook-
Nachrichten: je Sprache Teil 1 (fertige Versionen + aktuelle Version) und
Teil 2 (spätere Versionen); es wird nie neu gepostet. Nur Standardbibliothek.

Umgebung:
  GH_TOKEN                 Token mit Lesezugriff auf das Projekt
  DISCORD_ROADMAP_WEBHOOK  Webhook-URL (Secret)
  DISCORD_ROADMAP_MSGS     IDs der vier Nachrichten in Kanal-Reihenfolge:
                           DE Teil 1, DE Teil 2, EN Teil 1, EN Teil 2
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
# Sent as the description of an embed: a plain message allows only 2000
# characters, which the roadmap outgrew with five planned versions. One embed
# per language outgrew 4096 as well, hence two messages per language.
EMBED_LIMIT = 4096
ICON = {"Done": "✅", "In Progress": "🔧", "Todo": "▫️"}
PRIO_EN = {"Hoch": "high", "Mittel": "medium", "Niedrig": "low"}

# The version order comes from the project's "Version" field, so new versions
# show up without changing this script.
# Paged: a single page holds at most 100 items.
QUERY = """query($o:String!,$n:Int!,$after:String){user(login:$o){projectV2(number:$n){
  field(name:"Version"){... on ProjectV2SingleSelectField{options{name}}}
  items(first:100,after:$after){pageInfo{hasNextPage endCursor} nodes{
    content{... on DraftIssue{title} ... on Issue{title}}
    fieldValues(first:20){nodes{... on ProjectV2ItemFieldSingleSelectValue{name field{... on ProjectV2SingleSelectField{name}}}}}
  }}
}}}"""


def fetch_page(token, after):
    body = json.dumps({"query": QUERY, "variables": {"o": OWNER, "n": NUMBER, "after": after}}).encode()
    req = urllib.request.Request("https://api.github.com/graphql", body,
                                 {"Authorization": f"Bearer {token}", "Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        data = json.load(r)
    if data.get("errors"):
        sys.exit(f"GraphQL-Fehler: {data['errors']}")
    project = (data.get("data") or {}).get("user", {}).get("projectV2")
    if not project:
        sys.exit("Projekt nicht lesbar (Token ohne Projektzugriff?)")
    return project


def fetch_items(token):
    project = fetch_page(token, None)
    versions = [o["name"] for o in (project.get("field") or {}).get("options", [])]
    nodes = list(project["items"]["nodes"])
    while project["items"]["pageInfo"]["hasNextPage"]:
        project = fetch_page(token, project["items"]["pageInfo"]["endCursor"])
        nodes += project["items"]["nodes"]
    items = []
    for node in nodes:
        title = (node.get("content") or {}).get("title")
        fields = {v["field"]["name"]: v["name"] for v in node["fieldValues"]["nodes"] if v and v.get("field")}
        if title and fields.get("Version") in versions:
            items.append({"title": title, **fields})
    return versions, items


def split_title(title, lang):
    de, _, en = title.partition(" / ")
    return (de if lang == "de" else (en or de)).strip()


def release_marker(version, group):
    """The "vX veröffentlicht / released" entry of a finished version, else None.

    A released version is shown as this one line: Discord allows 2000 characters
    per message, and the done items are on the board anyway.
    """
    if not group or any(i.get("Status") != "Done" for i in group):
        return None
    return next((i for i in group if i["title"].startswith(version)), None)


def version_lines(version, group, lang):
    marker = release_marker(version, group)
    if marker:
        return [f"\n✅ **{split_title(marker['title'], lang)}**"]
    label = version if version != "Später" else ("Später" if lang == "de" else "Later")
    lines = [f"\n**{label}**"]
    for i in group:
        text = split_title(i["title"], lang)
        if version == "Später" and i.get("Priorität"):
            prio = i["Priorität"] if lang == "de" else PRIO_EN.get(i["Priorität"], i["Priorität"])
            text += f" · _{prio}_"
        lines.append(f"{ICON.get(i.get('Status'), '▫️')} {text}")
    return lines


def render(versions, items, lang):
    """Two texts per language: part 1 holds the released versions and the first
    unreleased one (the version being worked on), part 2 everything after it."""
    today = datetime.datetime.now(zoneinfo.ZoneInfo("Europe/Berlin")).date()
    if lang == "de":
        heads = (f"🗺️ **Roadmap** · Stand {today:%d.%m.%Y} · Teil 1/2: aktuell",
                 "🗺️ **Roadmap** · Teil 2/2: danach")
    else:
        heads = (f"🗺️ **Roadmap** · as of {today:%Y-%m-%d} · part 1/2: current",
                 "🗺️ **Roadmap** · part 2/2: next")
    parts = ([heads[0]], [heads[1]])
    current_seen = False
    for version in versions:
        group = [i for i in items if i.get("Version") == version]
        if not group:
            continue
        released = release_marker(version, group) is not None
        target = 1 if current_seen else 0
        if not released and not current_seen:
            current_seen = True
        parts[target].extend(version_lines(version, group, lang))
    legend = ("✅ fertig · 🔧 in Arbeit · ▫️ geplant" if lang == "de" else "✅ done · 🔧 in progress · ▫️ planned")
    link = (f"Komplette Roadmap: <{BOARD}>" if lang == "de" else f"Full roadmap: <{BOARD}>")
    for part in parts:
        part += ["", legend, link]
    texts = ["\n".join(p) for p in parts]
    for n, text in enumerate(texts, 1):
        if len(text) > EMBED_LIMIT:
            sys.exit(f"Nachricht ({lang}, Teil {n}) hat {len(text)} Zeichen, Discord erlaubt {EMBED_LIMIT}")
    return texts


def edit(webhook, message_id, content):
    # "content": "" clears the plain text of messages written before the switch to an embed.
    body = json.dumps({"content": "", "embeds": [{"description": content}],
                       "allowed_mentions": {"parse": []}}).encode()
    req = urllib.request.Request(f"{webhook}/messages/{message_id}", body,
                                 {"Content-Type": "application/json", "User-Agent": "3mf-roadmap-sync"}, method="PATCH")
    with urllib.request.urlopen(req, timeout=30) as r:
        r.read()


def main():
    versions, items = fetch_items(os.environ["GH_TOKEN"])
    if not items:
        # Never write an empty roadmap: the GitHub Actions default token can't see the
        # items of a user project.
        sys.exit("Keine Einträge gelesen - Token ohne Projektzugriff? (Secret ROADMAP_READ_TOKEN)")
    # Render everything first, so a too long text aborts before any message changes.
    texts = render(versions, items, "de") + render(versions, items, "en")
    if os.environ.get("DRY_RUN"):
        for text in texts:
            print(f"--- ({len(text)} Zeichen)\n{text}\n")
        return
    ids = [x.strip() for x in os.environ.get("DISCORD_ROADMAP_MSGS", "").split(",") if x.strip()]
    if len(ids) != 4:
        sys.exit("DISCORD_ROADMAP_MSGS braucht 4 Nachrichten-IDs: DE 1, DE 2, EN 1, EN 2")
    for message_id, text, name in zip(ids, texts, ("de 1", "de 2", "en 1", "en 2")):
        edit(os.environ["DISCORD_ROADMAP_WEBHOOK"], message_id, text)
        print(f"{name}: aktualisiert ({len(text)} Zeichen)")


if __name__ == "__main__":
    main()
