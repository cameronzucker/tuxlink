#!/usr/bin/env python3
"""Offline agentic eval harness for local Ollama models against Tuxlink's Elmer surface.
Faithful system prompt + 50-tool surface + mock tool returns. Full transcript + metrics.
Usage: harness.py MODEL PROMPT_IDS DEVICE TEMP THINK OUTDIR
  e.g. harness.py qwen2.5:14b-q8-ctx32768 1,2,3 igpu 0 none /home/administrator/evalout
"""
import json, sys, time, urllib.request, os, re

MODEL   = sys.argv[1]
PROMPTS = [int(x) for x in sys.argv[2].split(",")]
DEVICE  = sys.argv[3] if len(sys.argv) > 3 else "igpu"      # igpu | cpu
TEMP    = float(sys.argv[4]) if len(sys.argv) > 4 else 0.0
THINK   = sys.argv[5] if len(sys.argv) > 5 else "none"       # on | off | none
OUTDIR  = sys.argv[6] if len(sys.argv) > 6 else "/home/administrator/evalout"
HERE    = os.path.dirname(os.path.abspath(__file__))
os.makedirs(OUTDIR, exist_ok=True)

TOOLS = json.load(open(os.path.join(HERE, "tools.json")))

SYSTEM_PROMPT = (
"You are Elmer, an AI assistant embedded in Tuxlink — a Winlink and amateur-radio station "
"application — helping the licensed operator who is running this app. You have read-only tools "
"that report the operator's OWN station state: their location/grid (position_status), rig, modem, "
"mailbox, nearby stations, propagation and solar/space-weather. When a request depends on the "
"operator's location or station context, CALL the appropriate tool to get it — never ask the "
"operator for information Tuxlink already has (for example, never ask 'what is your location?'; "
"call position_status).\n\n"
"You can call tools as many times as a request needs, and call several in sequence, within one "
"reply. Many useful requests require exactly this: to answer 'which nearby VARA stations have the "
"best predicted path', call find_stations to get the candidates, then call predict_path for each "
"candidate, then rank and present the real results. Work the request with the tools — do NOT "
"refuse a multi-step task, cap how many tool calls you will make, or tell the operator to run the "
"tools themselves. Building a ranked list, table, or summary FROM real tool results is exactly "
"your job and is NOT fabrication.\n\n"
"You STAGE outbound traffic — a Winlink message (message_send), a Request Center inquiry "
"(catalog_send_inquiry), a GRIB weather-product request (grib_send_request), a form (send_form) "
"— into the local outbox. Staging is local and always available regardless of send authority. "
"The Winlink Request Center is a large on-demand catalog: call catalog_list to see everything the "
"operator can request, then stage the matching item(s) with catalog_send_inquiry.\n\n"
"Sending authority: you can connect and transmit when the operator has ARMED send authority. The "
"arm is a time-boxed grant — it IS the operator's Part 97 consent for that window. While armed, "
"you may iterate connect attempts autonomously. Egress is DENIED when send authority is disarmed, "
"has expired, or when the session is TAINTED. Do not treat a denial as an error to route around "
"— it means you are not currently authorized to transmit. The operator can abort at any time.\n\n"
"Do NOT claim a message has been sent or delivered when you have only staged it. NEVER fabricate "
"data a tool did not return — if a tool has not run or returned no real result, say so plainly "
"and never invent values, tables, or station lists out of thin air. This rule is about inventing "
"data you do not have; it does NOT mean avoiding tables or rankings built from real tool output.\n\n"
"Be concise and practical."
)

PROMPT_TEXT = {
1: "What are the 5 closest VARA gateways to me operating on the 80m band?",
2: ("What are the 5 closest VARA gateways to me operating on the 80m band? "
    "Which would I be most likely to connect to with a low ground-mounted delta loop?"),
3: ("Using the station finder, create a 24 hour rotating call list of 10 stations I'd be most "
    "likely to reach on an 80/40/20m tuneable delta loop from my current location, including "
    "which bands i should try at which times, in 2 hour increments. Then, compose that into a "
    "message for distribution and post it to the outbox. Finally, request operator permission to send."),
}

# ---- mock station directory (near grid DM43), realistic VARA-HF gateways ----
STATIONS = [
 {"callsign":"W7GW",  "grid":"DM43","band":"80m","freq_khz":3585.0,"distance_km":42,"last_heard_h":3,"modes":["vara-hf"]},
 {"callsign":"K7AZ",  "grid":"DM33","band":"80m","freq_khz":3592.0,"distance_km":88,"last_heard_h":9,"modes":["vara-hf"]},
 {"callsign":"N6XA",  "grid":"DM34","band":"80m","freq_khz":3578.0,"distance_km":150,"last_heard_h":1,"modes":["vara-hf"]},
 {"callsign":"W5RMS", "grid":"DM53","band":"80m","freq_khz":3590.0,"distance_km":205,"last_heard_h":20,"modes":["vara-hf"]},
 {"callsign":"KE7QRP","grid":"DM42","band":"80m","freq_khz":3583.0,"distance_km":260,"last_heard_h":5,"modes":["vara-hf"]},
 {"callsign":"AA7WL", "grid":"DM26","band":"80m","freq_khz":3588.0,"distance_km":410,"last_heard_h":30,"modes":["vara-hf"]},
 {"callsign":"KD6VER","grid":"DM13","band":"80m","freq_khz":3595.0,"distance_km":640,"last_heard_h":12,"modes":["vara-hf"]},
 {"callsign":"W7GW",  "grid":"DM43","band":"40m","freq_khz":7101.0,"distance_km":42,"last_heard_h":2,"modes":["vara-hf"]},
 {"callsign":"K7AZ",  "grid":"DM33","band":"40m","freq_khz":7104.0,"distance_km":88,"last_heard_h":4,"modes":["vara-hf"]},
 {"callsign":"NX7U",  "grid":"DM41","band":"40m","freq_khz":7098.0,"distance_km":175,"last_heard_h":6,"modes":["vara-hf"]},
 {"callsign":"W5RMS", "grid":"DM53","band":"40m","freq_khz":7103.0,"distance_km":205,"last_heard_h":8,"modes":["vara-hf"]},
 {"callsign":"KI7XYZ","grid":"DM09","band":"40m","freq_khz":7107.0,"distance_km":520,"last_heard_h":14,"modes":["vara-hf"]},
 {"callsign":"W7GW",  "grid":"DM43","band":"20m","freq_khz":14105.0,"distance_km":42,"last_heard_h":1,"modes":["vara-hf"]},
 {"callsign":"N6XA",  "grid":"DM34","band":"20m","freq_khz":14109.0,"distance_km":150,"last_heard_h":2,"modes":["vara-hf"]},
 {"callsign":"WA0RMS","grid":"EM48","band":"20m","freq_khz":14112.0,"distance_km":1180,"last_heard_h":7,"modes":["vara-hf"]},
]

_staged = [0]

def band_of(freqs):
    f = freqs[0] if freqs else 0
    if 3000 <= f <= 4500 or 3.0 <= f <= 4.5: return "80m"
    if 6500 <= f <= 7500 or 6.5 <= f <= 7.5: return "40m"
    if 13000 <= f <= 15000 or 13 <= f <= 15: return "20m"
    return "?"

def m_position_status(a):
    return {"grid":"DM43","gps_fix":"3D","source":"gps","precision":"4-char"}

def m_find_stations(a):
    bands = [b.lower() for b in a.get("bands",[])] if a.get("bands") else []
    modes = a.get("modes",[])
    res = []
    for s in STATIONS:
        if bands and s["band"] not in bands: continue
        if modes and not any(m in s["modes"] for m in modes): continue
        res.append(s)
    res.sort(key=lambda s: s["distance_km"])
    return {"count":len(res),"stations":res}

def m_predict_path(a):
    # diurnal reliability by band; UTC hours (local SW-US ~ UTC-7)
    freqs = a.get("frequencies_khz",[])
    rows = []
    for hr in range(0,24,2):
        night = (hr >= 2 and hr <= 14)   # ~ evening/night local
        row = {"utc_hour": hr}
        for f in freqs:
            b = band_of([f])
            if b == "80m":   rel = 85 if night else 25
            elif b == "40m": rel = 70 if (0<=hr<=16) else 55
            elif b == "20m": rel = 80 if (14<=hr<=23 or hr==0) else 30
            else: rel = 40
            row[f"{f}kHz_rel%"] = rel
        rows.append(row)
    return {"rx_grid":a.get("rx_grid"),"by_hour":rows,"note":"offline VOACAP estimate"}

def m_solar(a): return {"sfi":142,"a":6,"k":2,"ssn":88}

def m_message_send(a):
    _staged[0]+=1
    return {"staged_id":f"OUTBOX-{_staged[0]:04d}","status":"staged","folder":"outbox",
            "to":a.get("to"),"subject":a.get("subject")}

def m_catalog_list(a):
    return {"items":[{"id":"PROP_FORECAST.txt","category":"PROPAGATION","desc":"HF propagation forecast"},
                     {"id":"METAR_KPHX.txt","category":"METAR","desc":"Phoenix airport weather"}]}

def m_denied(a):
    return {"error":"DENIED","reason":"send authority is disarmed; operator must ARM to transmit (Part 97 consent)"}

EGRESS = {"cms_connect","verify_cms_connection","rig_tune","ardop_connect","ardop_b2f_exchange",
          "vara_b2f_exchange","packet_connect"}
MOCKS = {"position_status":m_position_status,"find_stations":m_find_stations,
         "predict_path":m_predict_path,"solar_conditions":m_solar,"message_send":m_message_send,
         "send_form":m_message_send,"catalog_send_inquiry":m_message_send,
         "grib_send_request":m_message_send,"catalog_list":m_catalog_list}

def run_tool(name, args):
    if name in EGRESS: return m_denied(args)
    fn = MOCKS.get(name)
    if fn: return fn(args)
    return {"ok":True,"note":f"{name} stub (no side effect in eval harness)"}

def chat(messages):
    body = {"model":MODEL,"stream":False,"messages":messages,"tools":TOOLS,
            "keep_alive":os.environ.get("KEEP_ALIVE","10m"),  # 0 on RAM-tight hosts; 10m keeps big models resident
            "options":{"temperature":TEMP,"num_ctx":32768}}
    if DEVICE=="cpu": body["options"]["num_gpu"]=0
    if THINK=="on": body["think"]=True
    if THINK=="off": body["think"]=False
    req=urllib.request.Request("http://localhost:11434/api/chat",
        data=json.dumps(body).encode(),headers={"Content-Type":"application/json"})
    return json.loads(urllib.request.urlopen(req,timeout=3600).read())

def nonascii_ratio(s):
    if not s: return 0.0
    return sum(1 for c in s if ord(c)>127)/len(s)

def run(pid, log):
    messages=[{"role":"system","content":SYSTEM_PROMPT},
              {"role":"user","content":PROMPT_TEXT[pid]}]
    tool_calls_total=0; tools_used=[]; final=""; eval_tokens=0
    t0=time.time(); reached=False; err=None
    MAX_TURNS=20
    log.write(f"\n{'='*70}\nMODEL={MODEL} DEVICE={DEVICE} TEMP={TEMP} THINK={THINK} PROMPT {pid}\n{'='*70}\n")
    log.write(f"USER: {PROMPT_TEXT[pid]}\n")
    for turn in range(MAX_TURNS):
        try:
            d=chat(messages)
        except Exception as e:
            err=str(e); log.write(f"\n[TURN {turn}] REQUEST ERROR: {err}\n"); break
        msg=d.get("message",{}) or {}
        eval_tokens+=d.get("eval_count") or 0
        think=msg.get("thinking") or ""
        content=msg.get("content") or ""
        tcs=msg.get("tool_calls") or []
        log.write(f"\n[TURN {turn}] eval={d.get('eval_count')} think_len={len(think)} content_len={len(content)} tool_calls={len(tcs)}\n")
        if think: log.write(f"  THINK: {think[:1500]}\n")
        if content: log.write(f"  CONTENT: {content[:3000]}\n")
        messages.append({"role":"assistant","content":content,
                         **({"tool_calls":tcs} if tcs else {}),
                         **({"thinking":think} if think else {})})
        if tcs:
            for tc in tcs:
                fn=tc.get("function",{})
                name=fn.get("name","?")
                args=fn.get("arguments",{})
                if isinstance(args,str):
                    try: args=json.loads(args)
                    except Exception: args={}
                tool_calls_total+=1; tools_used.append(name)
                result=run_tool(name,args)
                log.write(f"  -> CALL {name}({json.dumps(args)[:300]})\n")
                log.write(f"     RESULT {json.dumps(result)[:400]}\n")
                messages.append({"role":"tool","tool_name":name,"content":json.dumps(result)})
        else:
            final=content; reached=True; break
    dt=time.time()-t0
    summary={"model":MODEL,"device":DEVICE,"temp":TEMP,"think":THINK,"prompt":pid,
             "turns":turn+1,"tool_calls":tool_calls_total,
             "distinct_tools":sorted(set(tools_used)),"tool_sequence":tools_used,
             "reached_final":reached,"wall_s":round(dt,1),"eval_tokens":eval_tokens,
             "final_len":len(final),"nonascii_ratio":round(nonascii_ratio(final),3),
             "garbage":nonascii_ratio(final)>0.20 and len(final)>40,
             "error":err,"final_snippet":final[:800]}
    log.write(f"\nSUMMARY: {json.dumps(summary)}\n")
    return summary

def main():
    tag=re.sub(r'[^A-Za-z0-9._-]','_',f"{MODEL}__{DEVICE}_t{TEMP}_think-{THINK}")
    results=[]
    for pid in PROMPTS:
        lp=os.path.join(OUTDIR,f"{tag}__p{pid}.log")
        with open(lp,"w") as log:
            s=run(pid,log)
        results.append(s)
        with open(os.path.join(OUTDIR,"results.jsonl"),"a") as rf:
            rf.write(json.dumps(s)+"\n")
        print(json.dumps({k:s[k] for k in ("model","device","think","prompt","turns","tool_calls","distinct_tools","reached_final","wall_s","garbage","final_len")}))
    print("done", tag)

if __name__=="__main__":
    main()
