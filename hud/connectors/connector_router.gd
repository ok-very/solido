class_name ConnectorRouter
extends RefCounted

func route_all(
		ports: Dictionary,
		port_positions: Dictionary,
		nets: Array[HudNet],
		buses: Dictionary,
		unit_px: float,
) -> Array[Dictionary]:
	var sorted_nets := _sort_nets(nets)
	var results: Array[Dictionary] = []
	for net in sorted_nets:
		var result := _route_net(net, ports, port_positions, buses, unit_px)
		if result.size() > 0:
			results.append(result)
	return results


func _sort_nets(nets: Array[HudNet]) -> Array[HudNet]:
	var keyed: Array = []
	for net in nets:
		keyed.append([net, net.importance, net.net_id])
	keyed.sort_custom(
		func(a: Array, b: Array) -> bool:
			if a[1] != b[1]:
				return a[1] > b[1]
			return a[2] < b[2]
	)
	var result: Array[HudNet] = []
	for entry in keyed:
		result.append(entry[0])
	return result


func _route_net(
		net: HudNet,
		ports: Dictionary,
		port_positions: Dictionary,
		buses: Dictionary,
		unit_px: float,
) -> Dictionary:
	var from_port: HudPort = ports.get(net.from_port_id)
	var to_port: HudPort = ports.get(net.to_port_id)
	if from_port == null or to_port == null:
		return { }

	var from_pos: Vector2 = port_positions.get(net.from_port_id, Vector2.ZERO)
	var to_pos: Vector2 = port_positions.get(net.to_port_id, Vector2.ZERO)

	var segments: Array[Dictionary] = []
	if net.routing_mode == "BUS_BRANCH" and buses.has(net.bus_id):
		segments = _route_bus_branch(
			from_port,
			from_pos,
			to_port,
			to_pos,
			buses[net.bus_id],
			unit_px,
		)
	else:
		segments = _route_direct_arc(from_port, from_pos, to_port, to_pos, unit_px)

	return {
		"net_id": net.net_id,
		"segments": segments,
		"role": from_port.role,
		"importance": net.importance,
	}


func _route_direct_arc(
		from_port: HudPort,
		from_pos: Vector2,
		to_port: HudPort,
		to_pos: Vector2,
		unit_px: float,
) -> Array[Dictionary]:
	var points := _build_arc(from_pos, from_port.side, to_pos, to_port.side, unit_px)
	return [{ "points": points, "type": "arc" }]


func _route_bus_branch(
		from_port: HudPort,
		from_pos: Vector2,
		to_port: HudPort,
		to_pos: Vector2,
		bus: HudBus,
		unit_px: float,
) -> Array[Dictionary]:
	var lane := bus.pick_lane(from_port.port_id + ":" + to_port.port_id)
	var entry := bus.project_to_lane(lane, from_pos, unit_px)
	var exit_pt := bus.project_to_lane(lane, to_pos, unit_px)

	var bus_facing_from := _bus_side_facing(bus, from_pos)
	var arc1 := _build_arc(from_pos, from_port.side, entry, bus_facing_from, unit_px)

	var bus_seg := PackedVector2Array([entry, exit_pt])

	var bus_facing_to := _bus_side_facing(bus, to_pos)
	var arc2 := _build_arc(exit_pt, bus_facing_to, to_pos, to_port.side, unit_px)

	return [
		{ "points": arc1, "type": "arc" },
		{ "points": bus_seg, "type": "bus" },
		{ "points": arc2, "type": "arc" },
	]


func _build_arc(
		p0: Vector2,
		side0: String,
		p3: Vector2,
		side3: String,
		unit_px: float,
) -> PackedVector2Array:
	var curve := Curve2D.new()
	var d := p0.distance_to(p3)
	var min_t := 3.0 * unit_px
	var max_t := 16.0 * unit_px
	var t := clampf(d * 0.25, min_t, max_t)

	var dir0 := HudPort.dir_for_side(side0)
	var dir3 := HudPort.dir_for_side(side3)

	var p1 := p0 + dir0 * t
	var p2 := p3 - dir3 * t

	curve.add_point(p0, Vector2.ZERO, p1 - p0)
	curve.add_point(p3, p2 - p3, Vector2.ZERO)
	curve.bake_interval = 8.0
	return curve.get_baked_points()


func _bus_side_facing(bus: HudBus, source_pos: Vector2) -> String:
	if bus.orientation == HudBus.ORIENT_VERT:
		var bus_center_x := bus.rect.position.x + bus.rect.size.x * 0.5
		return "W" if source_pos.x < bus_center_x else "E"
	else:
		var bus_center_y := bus.rect.position.y + bus.rect.size.y * 0.5
		return "N" if source_pos.y < bus_center_y else "S"
