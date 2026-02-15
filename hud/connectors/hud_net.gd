class_name HudNet
extends RefCounted

const KIND_DATAFLOW := "DATAFLOW"
const KIND_HIGHLIGHT := "HIGHLIGHT"
const KIND_GROUP := "GROUP"
const KIND_WARNING := "WARNING"

var net_id: String
var kind: String
var importance: int
var from_port_id: String
var to_port_id: String
var routing_mode: String
var bus_id: String


static func create(
		p_net_id: String,
		p_kind: String,
		p_importance: int,
		p_from: String,
		p_to: String,
		p_routing_mode: String = "DIRECT_ARC",
		p_bus_id: String = "",
) -> HudNet:
	var net := HudNet.new()
	net.net_id = p_net_id
	net.kind = p_kind
	net.importance = p_importance
	net.from_port_id = p_from
	net.to_port_id = p_to
	net.routing_mode = p_routing_mode
	net.bus_id = p_bus_id
	return net


func get_visual_class() -> String:
	if importance >= 80:
		return "primary"
	elif importance >= 50:
		return "secondary"
	return "tertiary"
