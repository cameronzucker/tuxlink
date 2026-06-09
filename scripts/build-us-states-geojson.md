# Sourcing note: `src/request/us-states.geo.json`

The bundled US-state boundary dataset used by `latLonToUsState()` in
`src/request/geo.ts`.

## Source

Upstream file: PublicaMundi `MappingAPI` `us-states.json` — the widely-used
simplified US-states GeoJSON shipped with the Leaflet choropleth tutorial.

```
https://raw.githubusercontent.com/PublicaMundi/MappingAPI/master/data/geojson/us-states.json
```

It is a `FeatureCollection` of 52 features (all 50 states + the District of
Columbia + Puerto Rico). Geometry is already simplified for web display:
45 `Polygon` features and 7 `MultiPolygon` features (Alaska, Hawaii, and other
island/peninsula states). Coordinates are `[lon, lat]` (GeoJSON order).

## Transform applied (reproducible)

The upstream file carries a `name` (full state name) + `density` property. The
bundled copy is stripped + re-keyed for tuxlink:

1. Replace each feature's properties with a single `usps` 2-letter code
   (mapped from the full state name; matches the catalog's `WX_US_<ST>`
   category suffix).
2. Drop the `density` property.
3. Round all coordinates to 4 decimal places (~11 m precision — far finer than
   the simplified boundaries themselves; this is purely a size optimisation).
4. Serialize with no whitespace.

Result: ~72 KB (well under the ~150 KB bundle budget). The transform script is
inlined below; re-run it against a freshly-fetched upstream file to regenerate.

```python
import json
NAME2USPS = {  # full 52-entry name->USPS map; see geo.ts header for the catalog tie-in
  'Alabama':'AL','Alaska':'AK', ...  # (see git history of this file / the generating session)
}
src = json.load(open('us-states.json'))
def round_coords(c):
    if isinstance(c,(int,float)): return round(c,4)
    return [round_coords(x) for x in c]
out_feats = [{
    "type":"Feature",
    "properties":{"usps":NAME2USPS[f['properties']['name']]},
    "geometry":{"type":f['geometry']['type'],
                "coordinates":round_coords(f['geometry']['coordinates'])},
} for f in src['features']]
json.dump({"type":"FeatureCollection","features":out_feats},
          open('../src/request/us-states.geo.json','w'), separators=(',',':'))
```

## Accuracy caveat

These are display-simplified boundaries, not survey-grade. Points within ~1 km
of a state border may resolve to the neighbouring state. Callers should treat
the result as a best-effort regional hint, not an authoritative jurisdiction.
The request-center test points are chosen well inside each state.
