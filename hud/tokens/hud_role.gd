class_name HudRole
extends RefCounted

const NAV := 0
const EDIT := 1
const BLEND := 2
const TELEMETRY := 3
const ALERT := 4
const NEUTRAL := 5

const COUNT := 6

const NAMES := ["NAV", "EDIT", "BLEND", "TELEMETRY", "ALERT", "NEUTRAL"]


static func role_name(role: int) -> String:
	if role >= 0 and role < COUNT:
		return NAMES[role]
	return "UNKNOWN"
