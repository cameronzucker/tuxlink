import Point from '@mapbox/point-geometry';
import { PMTiles } from 'pmtiles';
import rBush from 'rbush';
import { Coords } from 'leaflet';
import { Flavor } from '@protomaps/basemaps';

type JsonValue = boolean | number | string | null | JsonArray | JsonObject;
interface JsonObject {
    [key: string]: JsonValue;
}
interface JsonArray extends Array<JsonValue> {
}
declare enum GeomType {
    Point = 1,
    Line = 2,
    Polygon = 3
}
interface Bbox {
    minX: number;
    minY: number;
    maxX: number;
    maxY: number;
}
interface Feature {
    readonly props: JsonObject;
    readonly bbox: Bbox;
    readonly geomType: GeomType;
    readonly geom: Point[][];
    readonly numVertices: number;
}
interface Zxy {
    readonly z: number;
    readonly x: number;
    readonly y: number;
}
declare function toIndex(c: Zxy): string;
interface TileSource {
    get(c: Zxy, tileSize: number): Promise<Map<string, Feature[]>>;
}
interface ZoomAbort {
    z: number;
    controller: AbortController;
}
declare class PmtilesSource implements TileSource {
    p: PMTiles;
    zoomaborts: ZoomAbort[];
    shouldCancelZooms: boolean;
    constructor(url: string | PMTiles, shouldCancelZooms: boolean);
    get(c: Zxy, tileSize: number): Promise<Map<string, Feature[]>>;
}
declare class ZxySource implements TileSource {
    url: string;
    zoomaborts: ZoomAbort[];
    shouldCancelZooms: boolean;
    constructor(url: string, shouldCancelZooms: boolean);
    get(c: Zxy, tileSize: number): Promise<Map<string, Feature[]>>;
}
interface CacheEntry {
    used: number;
    data: Map<string, Feature[]>;
}
interface PromiseOptions {
    resolve: (result: Map<string, Feature[]>) => void;
    reject: (e: Error) => void;
}
interface PickedFeature {
    feature: Feature;
    layerName: string;
}
declare function isInRing(point: Point, ring: Point[]): boolean;
declare function isCcw(ring: Point[]): boolean;
declare function pointInPolygon(point: Point, geom: Point[][]): boolean;
declare function pointMinDistToPoints(point: Point, geom: Point[][]): number;
declare function pointMinDistToLines(point: Point, geom: Point[][]): number;
declare class TileCache {
    source: TileSource;
    cache: Map<string, CacheEntry>;
    inflight: Map<string, PromiseOptions[]>;
    tileSize: number;
    constructor(source: TileSource, tileSize: number);
    get(c: Zxy): Promise<Map<string, Feature[]>>;
    queryFeatures(lng: number, lat: number, zoom: number, brushSize: number): PickedFeature[];
}

type AttrOption<T> = T | ((z: number, f?: Feature) => T);
declare class StringAttr<T extends string = string> {
    str: AttrOption<T>;
    perFeature: boolean;
    constructor(c: AttrOption<T> | undefined, defaultValue: T);
    get(z: number, f?: Feature): T;
}
declare class NumberAttr {
    value: AttrOption<number>;
    perFeature: boolean;
    constructor(c: AttrOption<number> | undefined, defaultValue?: number);
    get(z: number, f?: Feature): number;
}
interface TextAttrOptions {
    labelProps?: AttrOption<string[]>;
    textTransform?: AttrOption<string>;
}
declare class TextAttr {
    labelProps: AttrOption<string[]>;
    textTransform?: AttrOption<string>;
    constructor(options?: TextAttrOptions);
    get(z: number, f: Feature): string | undefined;
}
interface FontAttrOptions {
    font?: AttrOption<string>;
    fontFamily?: AttrOption<string>;
    fontSize?: AttrOption<number>;
    fontWeight?: AttrOption<number>;
    fontStyle?: AttrOption<string>;
}
declare class FontAttr {
    family?: AttrOption<string>;
    size?: AttrOption<number>;
    weight?: AttrOption<number>;
    style?: AttrOption<string>;
    font?: AttrOption<string>;
    constructor(options?: FontAttrOptions);
    get(z: number, f?: Feature): string;
}
declare class ArrayAttr<T = number> {
    value: AttrOption<T[]>;
    perFeature: boolean;
    constructor(c: AttrOption<T[]>, defaultValue?: T[]);
    get(z: number, f?: Feature): T[];
}

declare const Font: (name: string, url: string, weight: string) => Promise<FontFace>;
interface Sprite {
    x: number;
    y: number;
    w: number;
    h: number;
}
declare class Sheet {
    src: string;
    canvas: HTMLCanvasElement;
    mapping: Map<string, Sprite>;
    missingBox: Sprite;
    constructor(src: string);
    load(): Promise<this>;
    get(name: string): Sprite;
}

interface PaintSymbolizer {
    before?(ctx: CanvasRenderingContext2D, z: number): void;
    draw(ctx: CanvasRenderingContext2D, geom: Point[][], z: number, feature: Feature): void;
}
declare enum Justify {
    Left = 1,
    Center = 2,
    Right = 3
}
declare enum TextPlacements {
    N = 1,
    Ne = 2,
    E = 3,
    Se = 4,
    S = 5,
    Sw = 6,
    W = 7,
    Nw = 8
}
interface DrawExtra {
    justify: Justify;
}
interface LabelSymbolizer {
    place(layout: Layout, geom: Point[][], feature: Feature): Label[] | undefined;
}
declare const createPattern: (width: number, height: number, fn: (canvas: HTMLCanvasElement, ctx: CanvasRenderingContext2D) => void) => HTMLCanvasElement;
declare class PolygonSymbolizer implements PaintSymbolizer {
    pattern?: CanvasImageSource;
    fill: StringAttr;
    opacity: NumberAttr;
    stroke: StringAttr;
    width: NumberAttr;
    perFeature: boolean;
    doStroke: boolean;
    constructor(options: {
        pattern?: CanvasImageSource;
        fill?: AttrOption<string>;
        opacity?: AttrOption<number>;
        stroke?: AttrOption<string>;
        width?: AttrOption<number>;
        perFeature?: boolean;
    });
    before(ctx: CanvasRenderingContext2D, z: number): void;
    draw(ctx: CanvasRenderingContext2D, geom: Point[][], z: number, f: Feature): void;
}
declare function arr(base: number, a: number[]): (z: number) => number;
declare function exp(base: number, stops: number[][]): (z: number) => number;
type Stop = [number, number] | [number, string] | [number, boolean];
declare function step(output0: number | string | boolean, stops: Stop[]): (z: number) => number | string | boolean;
declare function linear(stops: number[][]): (z: number) => number;
declare class LineSymbolizer implements PaintSymbolizer {
    color: StringAttr;
    width: NumberAttr;
    opacity: NumberAttr;
    dash: ArrayAttr<number> | null;
    dashColor: StringAttr;
    dashWidth: NumberAttr;
    skip: boolean;
    perFeature: boolean;
    lineCap: StringAttr<CanvasLineCap>;
    lineJoin: StringAttr<CanvasLineJoin>;
    constructor(options: {
        color?: AttrOption<string>;
        width?: AttrOption<number>;
        opacity?: AttrOption<number>;
        dash?: number[];
        dashColor?: AttrOption<string>;
        dashWidth?: AttrOption<number>;
        skip?: boolean;
        perFeature?: boolean;
        lineCap?: AttrOption<CanvasLineCap>;
        lineJoin?: AttrOption<CanvasLineJoin>;
    });
    before(ctx: CanvasRenderingContext2D, z: number): void;
    draw(ctx: CanvasRenderingContext2D, geom: Point[][], z: number, f: Feature): void;
}
interface IconSymbolizerOptions {
    name: string;
    sheet: Sheet;
}
declare class IconSymbolizer implements LabelSymbolizer {
    name: string;
    sheet: Sheet;
    dpr: number;
    constructor(options: IconSymbolizerOptions);
    place(layout: Layout, geom: Point[][], feature: Feature): {
        anchor: Point;
        bboxes: {
            minX: number;
            minY: number;
            maxX: number;
            maxY: number;
        }[];
        draw: (ctx: CanvasRenderingContext2D) => void;
    }[];
}
declare class CircleSymbolizer implements LabelSymbolizer, PaintSymbolizer {
    radius: NumberAttr;
    fill: StringAttr;
    stroke: StringAttr;
    width: NumberAttr;
    opacity: NumberAttr;
    constructor(options: {
        radius?: AttrOption<number>;
        fill?: AttrOption<string>;
        stroke?: AttrOption<string>;
        width?: AttrOption<number>;
        opacity?: AttrOption<number>;
    });
    draw(ctx: CanvasRenderingContext2D, geom: Point[][], z: number, f: Feature): void;
    place(layout: Layout, geom: Point[][], feature: Feature): {
        anchor: Point;
        bboxes: {
            minX: number;
            minY: number;
            maxX: number;
            maxY: number;
        }[];
        draw: (ctx: CanvasRenderingContext2D) => void;
    }[];
}
declare class ShieldSymbolizer implements LabelSymbolizer {
    font: FontAttr;
    text: TextAttr;
    background: StringAttr;
    fill: StringAttr;
    padding: NumberAttr;
    constructor(options: {
        fill?: AttrOption<string>;
        background?: AttrOption<string>;
        padding?: AttrOption<number>;
    } & FontAttrOptions & TextAttrOptions);
    place(layout: Layout, geom: Point[][], f: Feature): {
        anchor: Point;
        bboxes: {
            minX: number;
            minY: number;
            maxX: number;
            maxY: number;
        }[];
        draw: (ctx: CanvasRenderingContext2D) => void;
    }[] | undefined;
}
declare class FlexSymbolizer implements LabelSymbolizer {
    list: LabelSymbolizer[];
    constructor(list: LabelSymbolizer[]);
    place(layout: Layout, geom: Point[][], feature: Feature): {
        anchor: Point;
        bboxes: Bbox[];
        draw: (ctx: CanvasRenderingContext2D) => void;
    }[] | undefined;
}
declare class GroupSymbolizer implements LabelSymbolizer {
    list: LabelSymbolizer[];
    constructor(list: LabelSymbolizer[]);
    place(layout: Layout, geom: Point[][], feature: Feature): {
        anchor: Point;
        bboxes: Bbox[];
        draw: (ctx: CanvasRenderingContext2D) => void;
    }[] | undefined;
}
declare class CenteredSymbolizer implements LabelSymbolizer {
    symbolizer: LabelSymbolizer;
    constructor(symbolizer: LabelSymbolizer);
    place(layout: Layout, geom: Point[][], feature: Feature): {
        anchor: Point;
        bboxes: {
            minX: number;
            maxX: number;
            minY: number;
            maxY: number;
        }[];
        draw: (ctx: CanvasRenderingContext2D) => void;
    }[] | undefined;
}
declare class Padding implements LabelSymbolizer {
    symbolizer: LabelSymbolizer;
    padding: NumberAttr;
    constructor(padding: number, symbolizer: LabelSymbolizer);
    place(layout: Layout, geom: Point[][], feature: Feature): Label[] | undefined;
}
interface TextSymbolizerOptions extends FontAttrOptions, TextAttrOptions {
    fill?: AttrOption<string>;
    stroke?: AttrOption<string>;
    width?: AttrOption<number>;
    lineHeight?: AttrOption<number>;
    letterSpacing?: AttrOption<number>;
    maxLineChars?: AttrOption<number>;
    justify?: Justify;
}
declare class TextSymbolizer implements LabelSymbolizer {
    font: FontAttr;
    text: TextAttr;
    fill: StringAttr;
    stroke: StringAttr;
    width: NumberAttr;
    lineHeight: NumberAttr;
    letterSpacing: NumberAttr;
    maxLineCodeUnits: NumberAttr;
    justify?: Justify;
    constructor(options: TextSymbolizerOptions);
    place(layout: Layout, geom: Point[][], feature: Feature): {
        anchor: Point;
        bboxes: {
            minX: number;
            minY: number;
            maxX: number;
            maxY: number;
        }[];
        draw: (ctx: CanvasRenderingContext2D, extra?: DrawExtra) => void;
    }[] | undefined;
}
declare class CenteredTextSymbolizer implements LabelSymbolizer {
    centered: LabelSymbolizer;
    constructor(options: TextSymbolizerOptions);
    place(layout: Layout, geom: Point[][], feature: Feature): Label[] | undefined;
}
interface OffsetSymbolizerValues {
    offsetX?: number;
    offsetY?: number;
    placements?: TextPlacements[];
    justify?: Justify;
}
type DataDrivenOffsetSymbolizer = (zoom: number, feature: Feature) => OffsetSymbolizerValues;
interface OffsetSymbolizerOptions {
    offsetX?: AttrOption<number>;
    offsetY?: AttrOption<number>;
    justify?: Justify;
    placements?: TextPlacements[];
    ddValues?: DataDrivenOffsetSymbolizer;
}
declare class OffsetSymbolizer implements LabelSymbolizer {
    symbolizer: LabelSymbolizer;
    offsetX: NumberAttr;
    offsetY: NumberAttr;
    justify?: Justify;
    placements: TextPlacements[];
    ddValues: DataDrivenOffsetSymbolizer;
    constructor(symbolizer: LabelSymbolizer, options: OffsetSymbolizerOptions);
    place(layout: Layout, geom: Point[][], feature: Feature): {
        anchor: Point;
        bboxes: {
            minX: number;
            minY: number;
            maxX: number;
            maxY: number;
        }[];
        draw: (ctx: CanvasRenderingContext2D) => void;
    }[] | undefined;
    computeXaxisOffset(offsetX: number, fb: Bbox, placement: TextPlacements): number;
    computeYaxisOffset(offsetY: number, fb: Bbox, placement: TextPlacements): number;
    computeJustify(fixedJustify: Justify | undefined, placement: TextPlacements): Justify;
}
declare class OffsetTextSymbolizer implements LabelSymbolizer {
    symbolizer: LabelSymbolizer;
    constructor(options: OffsetSymbolizerOptions & TextSymbolizerOptions);
    place(layout: Layout, geom: Point[][], feature: Feature): Label[] | undefined;
}
declare enum LineLabelPlacement {
    Above = 1,
    Center = 2,
    Below = 3
}
declare class LineLabelSymbolizer implements LabelSymbolizer {
    font: FontAttr;
    text: TextAttr;
    fill: StringAttr;
    stroke: StringAttr;
    width: NumberAttr;
    offset: NumberAttr;
    position: LineLabelPlacement;
    maxLabelCodeUnits: NumberAttr;
    repeatDistance: NumberAttr;
    constructor(options: {
        radius?: AttrOption<number>;
        fill?: AttrOption<string>;
        stroke?: AttrOption<string>;
        width?: AttrOption<number>;
        offset?: AttrOption<number>;
        maxLabelChars?: AttrOption<number>;
        repeatDistance?: AttrOption<number>;
        position?: LineLabelPlacement;
    } & TextAttrOptions & FontAttrOptions);
    place(layout: Layout, geom: Point[][], feature: Feature): {
        anchor: Point;
        bboxes: {
            minX: number;
            minY: number;
            maxX: number;
            maxY: number;
        }[];
        draw: (ctx: CanvasRenderingContext2D) => void;
        deduplicationKey: string;
        deduplicationDistance: number;
    }[] | undefined;
}

interface PreparedTile {
    z: number;
    origin: Point;
    data: Map<string, Feature[]>;
    scale: number;
    dim: number;
    dataTile: Zxy;
}
interface TileTransform {
    dataTile: Zxy;
    origin: Point;
    scale: number;
    dim: number;
}
declare const transformGeom: (geom: Array<Array<Point>>, scale: number, translate: Point) => Point[][];
declare const wrap: (val: number, z: number) => number;
declare class View {
    levelDiff: number;
    tileCache: TileCache;
    maxDataLevel: number;
    constructor(tileCache: TileCache, maxDataLevel: number, levelDiff: number);
    dataTilesForBounds(displayZoom: number, bounds: Bbox): Array<TileTransform>;
    dataTileForDisplayTile(displayTile: Zxy): TileTransform;
    getBbox(displayZoom: number, bounds: Bbox): Promise<Array<PreparedTile>>;
    getDisplayTile(displayTile: Zxy): Promise<PreparedTile>;
    queryFeatures(lng: number, lat: number, displayZoom: number, brushSize: number): PickedFeature[];
}
interface SourceOptions {
    levelDiff?: number;
    maxDataZoom?: number;
    url?: PMTiles | string;
    sources?: Record<string, SourceOptions>;
}
declare const sourcesToViews: (options: SourceOptions) => Map<string, View>;

type Filter = (zoom: number, feature: Feature) => boolean;
interface PaintRule {
    id?: string;
    minzoom?: number;
    maxzoom?: number;
    dataSource?: string;
    dataLayer: string;
    symbolizer: PaintSymbolizer;
    filter?: Filter;
}
declare function paint(ctx: CanvasRenderingContext2D, z: number, preparedTilemap: Map<string, PreparedTile[]>, labelData: Index | null, rules: PaintRule[], bbox: Bbox, origin: Point, clip: boolean, debug?: string): number;

type TileInvalidationCallback = (tiles: Set<string>) => void;
interface Label {
    anchor: Point;
    bboxes: Bbox[];
    draw: (ctx: CanvasRenderingContext2D, drawExtra?: DrawExtra) => void;
    deduplicationKey?: string;
    deduplicationDistance?: number;
}
interface IndexedLabel {
    anchor: Point;
    bboxes: Bbox[];
    draw: (ctx: CanvasRenderingContext2D) => void;
    order: number;
    tileKey: string;
    deduplicationKey?: string;
    deduplicationDistance?: number;
}
type TreeItem = Bbox & {
    indexedLabel: IndexedLabel;
};
interface Layout {
    index: Index;
    order: number;
    scratch: CanvasRenderingContext2D;
    zoom: number;
    overzoom: number;
}
interface LabelRule {
    id?: string;
    minzoom?: number;
    maxzoom?: number;
    dataSource?: string;
    dataLayer: string;
    symbolizer: LabelSymbolizer;
    filter?: Filter;
    visible?: boolean;
    sort?: (a: JsonObject, b: JsonObject) => number;
}
declare const covering: (displayZoom: number, tileWidth: number, bbox: Bbox) => {
    display: string;
    key: string;
}[];
declare class Index {
    tree: rBush<TreeItem>;
    current: Map<string, Set<IndexedLabel>>;
    dim: number;
    maxLabeledTiles: number;
    constructor(dim: number, maxLabeledTiles: number);
    hasPrefix(tileKey: string): boolean;
    has(tileKey: string): boolean;
    size(): number;
    keys(): IterableIterator<string>;
    searchBbox(bbox: Bbox, order: number): Set<IndexedLabel>;
    searchLabel(label: Label, order: number): Set<IndexedLabel>;
    bboxCollides(bbox: Bbox, order: number): boolean;
    labelCollides(label: Label, order: number): boolean;
    deduplicationCollides(label: Label): boolean;
    makeEntry(tileKey: string): void;
    insert(label: Label, order: number, tileKey: string): void;
    pruneOrNoop(keyAdded: string): void;
    pruneKey(keyToRemove: string): void;
    removeLabel(labelToRemove: IndexedLabel): void;
}
declare class Labeler {
    index: Index;
    z: number;
    scratch: CanvasRenderingContext2D;
    labelRules: LabelRule[];
    callback?: TileInvalidationCallback;
    constructor(z: number, scratch: CanvasRenderingContext2D, labelRules: LabelRule[], maxLabeledTiles: number, callback?: TileInvalidationCallback);
    private layout;
    private findInvalidatedTiles;
    add(preparedTilemap: Map<string, PreparedTile[]>): number;
}
declare class Labelers {
    labelers: Map<number, Labeler>;
    scratch: CanvasRenderingContext2D;
    labelRules: LabelRule[];
    maxLabeledTiles: number;
    callback: TileInvalidationCallback;
    constructor(scratch: CanvasRenderingContext2D, labelRules: LabelRule[], maxLabeledTiles: number, callback: TileInvalidationCallback);
    add(z: number, preparedTilemap: Map<string, PreparedTile[]>): number;
    getIndex(z: number): Index | undefined;
}

declare const getZoom: (degreesLng: number, cssPixels: number) => number;
interface StaticOptions {
    debug?: string;
    lang?: string;
    maxDataZoom?: number;
    url?: PMTiles | string;
    sources?: Record<string, SourceOptions>;
    paintRules?: PaintRule[];
    labelRules?: LabelRule[];
    backgroundColor?: string;
    flavor?: string;
}
declare class Static {
    paintRules: PaintRule[];
    labelRules: LabelRule[];
    views: Map<string, View>;
    debug?: string;
    backgroundColor?: string;
    constructor(options: StaticOptions);
    drawContext(ctx: CanvasRenderingContext2D, width: number, height: number, latlng: Point, displayZoom: number): Promise<{
        elapsed: number;
        project: (latlng: Point) => Point;
        unproject: (point: Point) => {
            lat: number;
            lng: number;
        };
    }>;
    drawCanvas(canvas: HTMLCanvasElement, latlng: Point, displayZoom: number, options?: StaticOptions): Promise<{
        elapsed: number;
        project: (latlng: Point) => Point;
        unproject: (point: Point) => {
            lat: number;
            lng: number;
        };
    } | undefined>;
    drawContextBounds(ctx: CanvasRenderingContext2D, topLeft: Point, bottomRight: Point, width: number, height: number): Promise<{
        elapsed: number;
        project: (latlng: Point) => Point;
        unproject: (point: Point) => {
            lat: number;
            lng: number;
        };
    }>;
    drawCanvasBounds(canvas: HTMLCanvasElement, topLeft: Point, bottomRight: Point, width: number, options?: StaticOptions): Promise<{
        elapsed: number;
        project: (latlng: Point) => Point;
        unproject: (point: Point) => {
            lat: number;
            lng: number;
        };
    } | undefined>;
}

type Status = {
    status: string;
    value?: unknown;
    reason: Error;
};
type DoneCallback = (error?: Error, tile?: HTMLElement) => void;
type KeyedHtmlCanvasElement = HTMLCanvasElement & {
    key: string;
};
interface LeafletLayerOptions extends L.GridLayerOptions {
    bounds?: L.LatLngBoundsExpression;
    attribution?: string;
    debug?: string;
    lang?: string;
    tileDelay?: number;
    noWrap?: boolean;
    paintRules?: PaintRule[];
    labelRules?: LabelRule[];
    tasks?: Promise<Status>[];
    maxDataZoom?: number;
    url?: PMTiles | string;
    sources?: Record<string, SourceOptions>;
    flavor?: string;
    backgroundColor?: string;
    devicePixelRatio?: number;
}
declare const leafletLayer: (options?: LeafletLayerOptions) => {
    [x: string]: any;
    paintRules: PaintRule[];
    labelRules: LabelRule[];
    backgroundColor?: string | undefined;
    devicePixelRatio: number;
    renderTile(coords: Coords, element: KeyedHtmlCanvasElement, key: string, done?: () => void): Promise<void>;
    rerenderTile(key: string): void;
    queryTileFeaturesDebug(lng: number, lat: number, brushSize?: number): Map<string, PickedFeature[]>;
    clearLayout(): void;
    rerenderTiles(): void;
    createTile(coords: Coords, showTile: DoneCallback): any;
    _removeTile(key: string): void;
};

declare const paintRules: (t: Flavor) => PaintRule[];
declare const labelRules: (t: Flavor, lang: string) => LabelRule[];

export { type Bbox, CenteredSymbolizer, CenteredTextSymbolizer, CircleSymbolizer, type DataDrivenOffsetSymbolizer, type DrawExtra, type Feature, type Filter, FlexSymbolizer, Font, GeomType, GroupSymbolizer, IconSymbolizer, type IconSymbolizerOptions, Index, type IndexedLabel, type JsonArray, type JsonObject, type JsonValue, Justify, type Label, type LabelRule, type LabelSymbolizer, Labeler, Labelers, type Layout, type LeafletLayerOptions, LineLabelPlacement, LineLabelSymbolizer, LineSymbolizer, OffsetSymbolizer, type OffsetSymbolizerOptions, type OffsetSymbolizerValues, OffsetTextSymbolizer, Padding, type PaintRule, type PaintSymbolizer, type PickedFeature, PmtilesSource, PolygonSymbolizer, type PreparedTile, Sheet, ShieldSymbolizer, type SourceOptions, Static, type Stop, TextPlacements, TextSymbolizer, type TextSymbolizerOptions, TileCache, type TileSource, type TileTransform, View, type Zxy, ZxySource, arr, covering, createPattern, exp, getZoom, isCcw, isInRing, labelRules, leafletLayer, linear, paint, paintRules, pointInPolygon, pointMinDistToLines, pointMinDistToPoints, sourcesToViews, step, toIndex, transformGeom, wrap };
