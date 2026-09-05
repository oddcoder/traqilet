#!/bin/sh
# Rebuild the coverage site's front page from `reports.tsv`. Both the publishing job and
# the nightly prune run this, so the table always says what is actually on the branch.
# Run it from the root of a `gh-pages` checkout.
set -eu

REPO=oddcoder/traqilet

cat >index.html <<'HTML'
<!doctype html>
<meta charset=utf-8>
<title>traqilet coverage</title>
<style>
body { font: 14px/1.5 system-ui, sans-serif; margin: 2rem auto; max-width: 50rem; }
table { border-collapse: collapse; width: 100%; }
th, td { border-bottom: 1px solid #d0d7de; padding: .4rem .6rem; text-align: left; }
th { font-weight: 600; }
td.pct { text-align: right; font-variant-numeric: tabular-nums; }
</style>
<h1>traqilet coverage</h1>
<p><a href=latest/>The newest report from <code>main</code></a>. Reports are kept for
thirty days.
<table>
<tr><th>Published<th>Source<th>Lines<th>Report<th>Run
HTML

# Newest first. The date leads every row, so a plain reverse sort is chronological.
sort -r reports.tsv | awk -v repo="$REPO" -F '\t' '
NF == 5 {
    date = $1; run = $2; kind = $3; label = $4; pct = $5
    if (kind == "pr") {
        source = "<a href=https://github.com/" repo "/pull/" label ">pull request #" label "</a>"
    } else {
        source = "<a href=https://github.com/" repo "/commit/" label "><code>" substr(label, 1, 8) "</code></a>"
    }
    printf "<tr><td>%s<td>%s<td class=pct>%s<td><a href=runs/%s/%s/>report</a>", \
        date, source, pct, date, run
    printf "<td><a href=https://github.com/%s/actions/runs/%s>%s</a>\n", repo, run, run
}' >>index.html

echo '</table>' >>index.html
