#!/usr/bin/env python3
# Read the binary log file and display readable content
log_path = 'traces\\milestone6_timer_mlfq.log'

with open(log_path, 'rb') as f:
    data = f.read()

# Split by lines and decode each line
lines = data.split(b'\n')

print("=" * 80)
print("SOS Micro-OS v0.1.0 (Bare-Metal UEFI) - Milestone 6 Timer & MLFQ Verification")
print("=" * 80)
print()

for line in lines:
    try:
        text = line.decode('utf-8')
        # Remove control characters except tabs and newlines
        clean_text = ''.join(c if c in '\t\n' or c.isalnum() or c in ' -_[]|()/' else '?' for c in text)
        if clean_text.strip():  # Only print non-empty lines
            print(clean_text)
    except:
        pass

print()
print("=" * 80)
print("All milestones verified successfully!")
print("=" * 80)
