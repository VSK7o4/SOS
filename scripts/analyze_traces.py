import os
import re
import csv

LOG_PATH = os.path.join("traces", "sos_full_validation.log")
RESULTS_DIR = "results"
os.makedirs(RESULTS_DIR, exist_ok=True)

with open(LOG_PATH, "rb") as f:
    raw = f.read().decode("ascii", errors="ignore")

clean_lines = []
for line in raw.splitlines():
    idx = line.find("[")
    if idx != -1 and any(tag in line[idx:] for tag in ["[INFO]", "[WARN]", "[ERROR]", "[TRACE]"]):
        clean_lines.append(line[idx:].strip())

clean_log_path = os.path.join("traces", "sos_clean_trace.txt")
with open(clean_log_path, "w", encoding="utf-8") as f:
    f.write("\n".join(clean_lines) + "\n")

trace_pattern = re.compile(r"\[TRACE\]\s+tick=(\d+)\s+\|\s+sub=([A-Z0-9_]+)\s+\|\s+event=([A-Z0-9_]+)\s+\|\s+(.*)")

rr_tasks = {}
mlfq_tasks = {}
test_cases = {}

for line in clean_lines:
    m = trace_pattern.match(line)
    if not m:
        continue
    tick, sub, event, details = int(m.group(1)), m.group(2), m.group(3), m.group(4)
    kv = dict(re.findall(r"([a-zA-Z0-9_]+)=([^\s]+)", details))

    if sub == "SCHED_RR" and event == "TASK_COMPLETE":
        name = kv.get("name", f"Task{kv.get('task_id')}")
        rr_tasks[name] = {
            "run": int(kv.get("total_run", 0)),
            "wait": int(kv.get("total_wait", 0)),
            "turnaround": int(kv.get("turnaround", 0)),
        }
    elif sub == "MLFQ" and event == "TASK_COMPLETE":
        name = kv.get("name", f"Task{kv.get('task_id')}")
        mlfq_tasks[name] = {
            "run": int(kv.get("run_ticks", 0)),
            "wait": int(kv.get("wait_ticks", 0)),
            "turnaround": int(kv.get("turnaround", 0)),
            "level": kv.get("final_level", "Q0"),
        }
    elif sub == "TEST":
        test_cases[event] = kv

csv_path = os.path.join(RESULTS_DIR, "metrics_summary.csv")
with open(csv_path, "w", newline="", encoding="utf-8") as f:
    writer = csv.writer(f)
    writer.writerow(["Scheduler", "Task", "Run_Ticks", "Wait_Ticks", "Turnaround_Ticks", "Final_Queue"])
    for name, d in rr_tasks.items():
        writer.writerow(["RoundRobin_Q2", name, d["run"], d["wait"], d["turnaround"], "RR_FIFO"])
    for name, d in mlfq_tasks.items():
        writer.writerow(["MLFQ_3Level", name, d["run"], d["wait"], d["turnaround"], d["level"]])

def write_svg_bar_chart(filename, title, labels, series1, series2, label1, label2, color1, color2):
    width, height = 760, 420
    margin_left, margin_bottom, margin_top = 70, 80, 60
    plot_w = width - margin_left - 40
    plot_h = height - margin_bottom - margin_top
    max_val = max(max(series1, default=1), max(series2, default=1), 1)
    n = max(len(labels), 1)
    group_w = plot_w / n
    bar_w = min(group_w * 0.32, 42)

    svg = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" style="background:#ffffff;font-family:Segoe UI,Arial,sans-serif">',
        f'<text x="{width/2}" y="34" text-anchor="middle" font-size="16" font-weight="bold" fill="#1e293b">{title}</text>',
        f'<line x1="{margin_left}" y1="{margin_top}" x2="{margin_left}" y2="{margin_top+plot_h}" stroke="#64748b" stroke-width="1.5"/>',
        f'<line x1="{margin_left}" y1="{margin_top+plot_h}" x2="{margin_left+plot_w}" y2="{margin_top+plot_h}" stroke="#64748b" stroke-width="1.5"/>'
    ]

    for step in range(5):
        val = int(round(max_val * step / 4))
        y = margin_top + plot_h - (plot_h * step / 4)
        svg.append(f'<line x1="{margin_left}" y1="{y}" x2="{margin_left+plot_w}" y2="{y}" stroke="#e2e8f0" stroke-dasharray="4,4"/>')
        svg.append(f'<text x="{margin_left-10}" y="{y+4}" text-anchor="end" font-size="11" fill="#475569">{val}</text>')

    for i, lbl in enumerate(labels):
        cx = margin_left + (i + 0.5) * group_w
        h1 = (series1[i] / max_val) * plot_h
        h2 = (series2[i] / max_val) * plot_h
        y1 = margin_top + plot_h - h1
        y2 = margin_top + plot_h - h2
        svg.append(f'<rect x="{cx - bar_w - 2}" y="{y1}" width="{bar_w}" height="{h1}" fill="{color1}" rx="3"/>')
        svg.append(f'<text x="{cx - bar_w/2 - 2}" y="{y1 - 5}" text-anchor="middle" font-size="10" fill="#1e293b">{series1[i]}</text>')
        svg.append(f'<rect x="{cx + 2}" y="{y2}" width="{bar_w}" height="{h2}" fill="{color2}" rx="3"/>')
        svg.append(f'<text x="{cx + bar_w/2 + 2}" y="{y2 - 5}" text-anchor="middle" font-size="10" fill="#1e293b">{series2[i]}</text>')
        svg.append(f'<text x="{cx}" y="{margin_top+plot_h+22}" text-anchor="middle" font-size="11" fill="#1e293b">{lbl}</text>')

    svg.append(f'<rect x="{width-240}" y="48" width="12" height="12" fill="{color1}"/>')
    svg.append(f'<text x="{width-222}" y="58" font-size="11" fill="#334155">{label1}</text>')
    svg.append(f'<rect x="{width-125}" y="48" width="12" height="12" fill="{color2}"/>')
    svg.append(f'<text x="{width-107}" y="58" font-size="11" fill="#334155">{label2}</text>')
    svg.append('</svg>')

    with open(os.path.join(RESULTS_DIR, filename), "w", encoding="utf-8") as f:
        f.write("\n".join(svg))

if rr_tasks:
    names = list(rr_tasks.keys())
    write_svg_bar_chart(
        "round_robin_metrics.svg",
        "Core Layer 5: Round Robin Scheduler (Quantum = 2 Ticks)",
        names,
        [rr_tasks[n]["wait"] for n in names],
        [rr_tasks[n]["turnaround"] for n in names],
        "Wait Ticks",
        "Turnaround Ticks",
        "#3b82f6",
        "#1d4ed8",
    )

if mlfq_tasks:
    names = list(mlfq_tasks.keys())
    write_svg_bar_chart(
        "mlfq_specialization_metrics.svg",
        "Track A Specialization: 3-Level MLFQ Scheduler with Aging",
        names,
        [mlfq_tasks[n]["wait"] for n in names],
        [mlfq_tasks[n]["turnaround"] for n in names],
        "Wait Ticks",
        "Turnaround Ticks",
        "#10b981",
        "#047857",
    )

tc_labels = ["TC1 (2 Tasks)", "TC2 (5 Tasks)", "TC3 (Q=1 Boundary)", "TC4 (12 Tasks Stress)"]
tc_keys = ["TC1_MIN_CONFIG", "TC2_MULTI_TASKS", "TC3_BOUNDARY_QUANTUM1", "TC4_STRESS_12TASKS"]
tc_ticks = [int(test_cases.get(k, {}).get("total_ticks", 0)) for k in tc_keys]
tc_sw = [int(test_cases.get(k, {}).get("switches", 0)) for k in tc_keys]

write_svg_bar_chart(
    "test_suite_scaling.svg",
    "SOS Validation Suite (TC1 - TC4): Execution Ticks & Context Switches",
    tc_labels,
    tc_ticks,
    tc_sw,
    "Total Ticks",
    "Context Switches",
    "#8b5cf6",
    "#f59e0b",
)

print(f"Parsed {len(clean_lines)} structured kernel log lines.")
print(f"Saved clean trace to: {clean_log_path}")
print(f"Saved CSV metrics to: {csv_path}")
print("Generated SVG charts in results/: round_robin_metrics.svg, mlfq_specialization_metrics.svg, test_suite_scaling.svg")
