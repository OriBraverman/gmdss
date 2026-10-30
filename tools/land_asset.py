"""Makes app/assets/chart/land.txt from Natural Earth `ne_50m_land.geojson` (public domain,
https://www.naturalearthdata.com/): one ring per line, `lon lat` pairs in hundredths of a degree.
Holes are rings too; the chart fills even-odd.

usage: python land_asset.py ne_50m_land.geojson app/assets/chart/land.txt
"""
import json
import sys

src, dst = sys.argv[1], sys.argv[2]
features = json.load(open(src, encoding='utf-8'))['features']
lines = []
for f in features:
    g = f['geometry']
    polygons = [g['coordinates']] if g['type'] == 'Polygon' else g['coordinates']
    for polygon in polygons:
        for ring in polygon:
            pts = []
            for lon, lat in ring:
                p = (round(lon * 100), round(lat * 100))
                if not pts or pts[-1] != p:
                    pts.append(p)
            if len(pts) >= 4:
                lines.append(' '.join(f'{x} {y}' for x, y in pts))
with open(dst, 'w', encoding='ascii', newline='\n') as out:
    out.write('# Natural Earth 1:50m land (public domain), lon lat in 1/100 degree, one ring per line\n')
    out.write('\n'.join(lines) + '\n')
print(len(lines), 'rings', sum(l.count(' ') + 1 for l in lines) // 2, 'points')
