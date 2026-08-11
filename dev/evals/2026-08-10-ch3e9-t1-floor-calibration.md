# ch3e9 step-2 validation — 44-query floor + threshold calibration (native)

The first run of the REAL classifier path: `tuxlink-classify`'s candle
bge-small backend over the enriched full-catalog index, against the T1
spike's 44 labeled queries. This run is simultaneously the step-2 accuracy
floor, the calibration that produced the shipped
`resources/catalog/classify-thresholds.json`, and the confirming-candle
runtime measurement ADR 0030 required before quoting native numbers as fact.

## Provenance

- Host: R2 (i3-N305) — the acceptance platform; `RAYON_NUM_THREADS=4`
- Code: branch `bd-tuxlink-ch3e9/t1-classifier` @ d5415b13 (fresh clone
  `~/tuxlink-ch3e9-build`), `cargo run --release --example eval_floor`
- Weights: HF snapshot `5c38ec7c405ec4b44b94cc5a9bb96e735b38267a` of
  BAAI/bge-small-en-v1.5 (the same snapshot the 2026-08-09 python spike
  measured; its `1_Pooling/config.json` says `pooling_mode_cls_token: true`,
  matching the backend's `Pooling::Cls`)
- Unit tests: 11/11 green on the same host/commit (`cargo test -p
  tuxlink-classify --locked`), incl. both asset drift gates
- Raw log: `~/tuxlink-ch3e9-eval.log` on R2 (per-query table reproduced
  below)

## Results

**Top-1 floor: 35/36 = 97.2%** (item 29/30, section 6/6) — identical to the
python spike's 97.2%, and the identical single miss:

- `buoy-sf` ("buoy report near san francisco") top-1s an Atlantic buoy.
  This is the coordinates probe the T1 spike already adjudicated: geo is
  the wrong job for embeddings. The enriched index now carries parsed
  lat/lon on 573 entries (NDBC46026 = 37.75,-122.84), so the deterministic
  caller-side nearest-point answer is available at step-3 wiring; the
  embedding path is not expected to solve it.

**Reject gap: SEPARATED.** none-class max 0.6165 (`none-capital`) vs
true-class min 0.6869 (`vague-wx`). All five none-class queries (incl. the
near-domain `none-ardop` at 0.6165) fall under the floor.

**Margins:** answer-class median 0.0522 vs ask-class median 0.0068. The
printed OVERLAP flag (answer-min 0.0103 < ask-max 0.0304) is the expected
generic-query effect, not a defect: every answer-labeled query under the
chosen margin (`az-generic` 0.0004, `sat-pix` 0.0014, `nv-reno` 0.0041,
`prop-generic` 0.0091, `buoy-41009` 0.0103) is a broad ask whose top
candidates are same-section siblings — the Ambiguous verdict there produces
exactly the "which product do you want?" disambiguation the originating
issue described as correct behavior. Specific queries sit above the margin
(tightest: `aurora-tonight` 0.0230).

**Locality (the design-story replication):** bare "pull the weather for my
local area" → top-3 all WX_US_OR, margin 0.0026 (ask). With
`(operator location: grid DM43, Arizona, United States)` appended → top-3
all WX_US_AZ, margin 0.0003 (ask WHICH Arizona product). Context resolves
the section; the residual ask is the designed behavior. Matches the python
spike's quantified result through the native path.

**Native runtime (now quotable):** model load 185ms; per-query encode
median **13.3ms** (matches the spike's ~14ms isolated probe — the python
150ms in-harness numbers were indeed measurement artifact); one-time index
build **125.5s** for 1,477 items (85ms/item — slower than python's batched
5.8ms/item; the serde-persistable index exists to absorb this as a
per-catalog-change cost, and batch-path optimization is available headroom,
not a step-2 blocker).

## Shipped thresholds (measured midpoints)

```json
{"winlink-catalog/bge-small-en-v1.5/enriched-v1":{"reject_floor":0.652,"ask_margin":0.020}}
```

With these applied to the 44 labels: all none-class → NoMatch; all
ambig-class → Ambiguous; one label-level miss (`grib-generic` margin 0.0304
→ Match on CUSTOM.GRIB, the custom-GRIB help doc — the same case the
parseability spike's adjudication called defensible).

## Per-query table (id / grade / top1 / score / margin / top-5)

```text
aurora-tonight	HIT	AUR_TONIGHT	0.7621	0.0230	AUR_TONIGHT,AUR_TOMORROW,US.RAD.WFNAK,AK_ZON_ANC2,US.RAD.EALAK
aurora-tomorrow-para	HIT	AUR_TOMORROW	0.7615	0.0587	AUR_TOMORROW,AUR_TONIGHT,GLAPXRBL.JPG,FQCN37.CWUL,ME_ZON_PENON
ice-west-ak	HIT	FZAK80	0.8527	0.0655	FZAK80,FZAK80.PAFC,PKZ199.TXT,SRUS88.PAFC,FZPN01.KWBC
metar-nice	HIT	FRA_NIC_ICAO	0.7540	0.0507	FRA_NIC_ICAO,FRA_HYE_ICAO,FRA_BEZ_ICAO,FRA_MAR_ICAO,FRA_CAL_ICAO
metar-dubrovnik-para	HIT	CRO_DUB_ICAO	0.7961	0.0599	CRO_DUB_ICAO,CRO_RIJ_ICAO,SLO_POR_ICAO,ITA_VEN_ICAO,GRE_PRE_ICAO
fl-auxcomm	HIT	FL_AUX	0.8063	0.0694	FL_AUX,FL_P2P_NET,TX_D16_RACES,TX_RACES,TX_MOCO_ARES
tx-races	HIT	TX_RACES	0.7801	0.0262	TX_RACES,TX_D16_RACES,TX_MOCO_ARES,FL_P2P_NET,FL_AUX
roatan-15day	HIT	ROATAN	0.7631	0.1053	ROATAN,GRACIAS,CATACAMAS,JUTICALPA,PUERTOLEMPIR
hf-nets-cruisers	HIT	HF_NETS	0.7805	0.1190	HF_NETS,FZNT24.KWNM,FZNT23.KWNM,HI_SEAS_FOR,FQCN33.CWHX
keps-iss	HIT	ISS.2KEPS	0.8163	0.0709	ISS.2KEPS,AMSAT.KEPS,VIS.2KEPS,HAM.2KEPS,WX.2KEPS
tle-ham	HIT	HAM.2KEPS	0.7749	0.0432	HAM.2KEPS,WX.2KEPS,AMSAT.KEPS,VIS.2KEPS,ISS.2KEPS
keps-wxsats	HIT	WX.2KEPS	0.7415	0.0237	WX.2KEPS,HAM.2KEPS,AMSAT.KEPS,VIS.2KEPS,ISS.2KEPS
wwv-flux	HIT	PROP_WWV	0.8058	0.0864	PROP_WWV,PROP.27DO,PROP_SGAS_27,PROP_SGAS,PROP_SOLWIND
drap	HIT	PROP_DRAP	0.7430	0.0668	PROP_DRAP,PROP_SOLWIND,NAR420.JPG,PAC_ISL0000,PAC_ISL1200
spacewx-3day	HIT	PROP_3DAY	0.7586	0.0356	PROP_3DAY,PROP_3DPROB,PROP.27DO,PROP_SGAS_27,NE_PPBM50
cat-update-we	HIT	UPDA_CAT_WE	0.7861	0.0235	UPDA_CAT_WE,UPDATE_CAT,UPDA_CAT_AIR,ACCEPTLIST,FIRM_UPDATE
custom-grib-help	HIT	CUSTOM.GRIB	0.8322	0.1121	CUSTOM.GRIB,MAXSAEA_GRIB,MA_XIV.HSPAC,FL_P2P_NET,DWD_DE_FAX
change-call	HIT	CHANGE_CALL	0.7322	0.1335	CHANGE_CALL,P3FREQLIST,FIRM_UPDATE,PRECEDENCE,ACCEPTLIST
buoy-41009	HIT	NDBC41009	0.8490	0.0103	NDBC41009,NDBC41001,NDBC41010,NDBC41002,NDBC41008
hap-perth	HIT	AUS_HAP_PER	0.7568	0.0235	AUS_HAP_PER,AUS_HAP_CAN,AUS_HAP_SYD,NZ_HAP_AUC,AUS_HAP_MEL
highseas-tropatl	HIT	FZNT02.KNHC	0.7969	0.0362	FZNT02.KNHC,FZPN03.KNHC,FZNT01.KWBC,INMARSAT_HF,ACCA62.TJSG
efd-caribbean	HIT	US.CARIBBEAN	0.7588	0.0243	US.CARIBBEAN,AMZ046.TXT,AMZ047.TXT,AMZ050.TXT,AMZ053.TXT
az-zone-se	HIT	AZ_ZON_SE	0.7686	0.0244	AZ_ZON_SE,AZ_TAB_SE,CA_ZON_SESWA,AZ_ZON_NOFLA,AZ_ZON_SW
nv-afd	HIT	NV_DIS_NV	0.7745	0.0522	NV_DIS_NV,NV_ZON_WESNE,NV_TAB_NOCEN,NV_TAB_MJGB,CA_ZON_WESNE
ionogram-rome	HIT	IONOG_ROME	0.7810	0.0778	IONOG_ROME,IONOG_TUCU,IONO_GLO_E,ITA_BAR_ICAO,IONO_GLO_W
conv-outlook	HIT	CONV_OUT	0.7319	0.0712	CONV_OUT,ABNT20.KNHC,OR_ZON_GRROV,ACPN50.PHFO,US.RAD.LINY
ice-hudson	HIT	FICN15CWIS	0.7555	0.0506	FICN15CWIS,FQCN26.CWNT,FICN16CWIS,FZAK80,FICN10CWIS
spacewx-advisory	HIT	PROP_ADVIS	0.7103	0.0278	PROP_ADVIS,PROP.27DO,PROP_3DAY,PROP_SGAS_27,TN_HAZ_MEMP
local-wx	ambig	OR_ZON_LAKCE	0.7147	0.0026	OR_ZON_LAKCE,OR_ZON_KLALA,OR_ZON_CLAT,OR_ZON_NORCE,OR_ZON_DOUEF
local-wx-ctx	HIT	AZ_ZON_NOFLA	0.7182	0.0068	AZ_ZON_NOFLA,AZ_ZON_SW,AZ_TAB_SW,AZ_ZON_SE,AZ_TAB_NORT
vague-wx	ambig	WMEDHI_72FOR	0.6869	0.0016	WMEDHI_72FOR,OR_ZON_CW500,WMEDHI_48FOR,WMEDHI_24FOR,OR_ZON_CCI84
prop-generic	HIT	PROP_3DAY	0.7322	0.0091	PROP_3DAY,PROP_3DPROB,PROP_SOLWIND,PROPWKHI,PROP3DNOAA
winlink-help	HIT	METAR	0.7464	0.0123	METAR,DIDUNO,USER.OPTIONS,ACCEPTLIST,WL2K.QTH
grib-generic	ambig	CUSTOM.GRIB	0.7535	0.0304	CUSTOM.GRIB,MAXSAEA_GRIB,GLSTATE.JPG,US.RAD.GRLAK,GFS_UV300
sat-pix	HIT	ALVS.JPG	0.7510	0.0014	ALVS.JPG,SPLSTATE.JPG,SCSTATE.JPG,GEIR.JPG,SESTATE.JPG
wxfax-atl	HIT	PJAM98.TIF	0.8176	0.0146	PJAM98.TIF,ATSA00Z.GIF,PPAE50.TIF,PPAE00.TIF,PPAI50.TIF
buoy-sf	MISS	NDBC44033	0.7379	0.0091	NDBC44033,NDBC46041,NDBC46047,NDBC46028,NDBC51002
az-generic	HIT	AZ_ZON_NOFLA	0.7012	0.0004	AZ_ZON_NOFLA,AZ_ZON_SW,AZ_ZON_SE,AZ_TAB_SW,AZ_TAB_NORT
nv-reno	HIT	NV_ZON_WESNE	0.7191	0.0041	NV_ZON_WESNE,NV_TAB_RENO,CA_ZON_WESNE,NV_TAB_NOCEN,NV_TAB_MJGB
none-pizza	none	WL2K.QTH	0.5478	0.0242	WL2K.QTH,P3FREQLIST,HISTATE.JPG,PACHI_IR.JPG,FZNT23.KWNM
none-capital	none	FRA_BEZ_ICAO	0.6165	0.0305	FRA_BEZ_ICAO,FRA_NIC_ICAO,FRE_POLY_CE,FRE_POLY_ME,FRA_CAL_ICAO
none-music	none	US_AMAT_FREQ	0.4962	0.0166	US_AMAT_FREQ,P3FREQLIST,ACCEPTLIST,GLAPXRBL.JPG,SCSTATE.JPG
none-email	none	P3FREQLIST	0.5619	0.0255	P3FREQLIST,FIRM_UPDATE,US_AMAT_FREQ,IRIDIUM,WL2K.QTH
none-ardop	none	PUB_ARDOP	0.6165	0.0274	PUB_ARDOP,FL_P2P_NET,FL_AUX,TX_RACES,P3FREQLIST
```

Session: moss-tamarack-taiga, 2026-08-10 evening AZT.
