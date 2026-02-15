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
			net.importance,
			unit_px,
		)
	else:
		segments = _route_direct_arc(
			from_port,
			from_pos,
			to_port,
			to_pos,
			net.importance,
			unit_px,
		)

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
		importance: int,
		unit_px: float,
) -> Array[Dictionary]:
	var points := _build_rectilinear(
		from_pos,
		from_port.side,
		to_pos,
		to_port.side,
		importance,
		unit_px,
	)
	return [{ "points": points, "type": "arc" }]


func _route_bus_branch(
		from_port: HudPort,
		from_pos: Vector2,
		to_port: HudPort,
		to_pos: Vector2,
		bus: HudBus,
		importance: int,
		unit_px: float,
) -> Array[Dictionary]:
	var lane := bus.pick_lane(from_port.port_id + ":" + to_port.port_id)
	var entry := bus.project_to_lane(lane, from_pos, unit_px)
	var exit_pt := bus.project_to_lane(lane, to_pos, unit_px)

	var bus_facing_from := _bus_side_facing(bus, from_pos)
	var seg1 := _build_rectilinear(
		from_pos,
		from_port.side,
		entry,
		bus_facing_from,
		importance,
		unit_px,
	)

	var bus_seg := PackedVector2Array([entry, exit_pt])

	var bus_facing_to := _bus_side_facing(bus, to_pos)
	var seg2 := _build_rectilinear(
		exit_pt,
		bus_facing_to,
		to_pos,
		to_port.side,
		importance,
		unit_px,
	)

	return [
		{ "points": seg1, "type": "arc" },
		{ "points": bus_seg, "type": "bus" },
		{ "points": seg2, "type": "arc" },
	]

# --- Rectilinear routing ---


func _build_rectilinear(
		p0: Vector2,
		side0: String,
		p3: Vector2,
		side3: String,
		importance: int,
		unit_px: float,
) -> PackedVector2Array:
	if p0.distance_to(p3) < 1.0:
		return PackedVector2Array([p0, p3])

	var dir0 := HudPort.dir_for_side(side0)
	var dir3 := HudPort.dir_for_side(side3)
	var radius := _elbow_radius_for_importance(importance, unit_px)

	if side0 == side3:
		return _build_u_route(p0, dir0, p3, dir3, radius, unit_px)

	var is_h0 := dir0.x != 0.0
	var is_h3 := dir3.x != 0.0
	if is_h0 == is_h3:
		return _build_z_route(p0, dir0, p3, dir3, radius)
	else:
		return _build_l_route(p0, dir0, p3, dir3, radius)


func _build_l_route(
		p0: Vector2,
		dir0: Vector2,
		p3: Vector2,
		dir3: Vector2,
		radius: float,
) -> PackedVector2Array:
	var is_h0 := dir0.x != 0.0
	var corner: Vector2
	if is_h0:
		corner = Vector2(p3.x, p0.y)
	else:
		corner = Vector2(p0.x, p3.y)

	if (corner - p0).dot(dir0) <= 0.0:
		return _build_z_route(p0, dir0, p3, dir3, radius)

	var seg1_len := p0.distance_to(corner)
	var seg2_len := corner.distance_to(p3)
	var r := minf(radius, minf(seg1_len, seg2_len) * 0.5)

	if r < 1.0:
		return PackedVector2Array([p0, p3])

	var d_in := (corner - p0).normalized()
	var d_out := (p3 - corner).normalized()

	var points := PackedVector2Array()
	points.append(p0)
	points.append_array(_build_elbow_points(corner, d_in, d_out, r))
	points.append(p3)
	return points


func _build_z_route(
		p0: Vector2,
		dir0: Vector2,
		p3: Vector2,
		_dir3: Vector2,
		radius: float,
) -> PackedVector2Array:
	var is_h0 := dir0.x != 0.0
	var corner1: Vector2
	var corner2: Vector2

	if is_h0:
		if absf(p3.y - p0.y) < 1.0:
			return PackedVector2Array([p0, p3])
		var mid_x := (p0.x + p3.x) * 0.5
		corner1 = Vector2(mid_x, p0.y)
		corner2 = Vector2(mid_x, p3.y)
	else:
		if absf(p3.x - p0.x) < 1.0:
			return PackedVector2Array([p0, p3])
		var mid_y := (p0.y + p3.y) * 0.5
		corner1 = Vector2(p0.x, mid_y)
		corner2 = Vector2(p3.x, mid_y)

	var seg1_len := p0.distance_to(corner1)
	var seg_mid_len := corner1.distance_to(corner2)
	var seg3_len := corner2.distance_to(p3)
	var r := minf(radius, minf(seg1_len, minf(seg_mid_len * 0.5, seg3_len)))

	if r < 1.0:
		return PackedVector2Array([p0, corner1, corner2, p3])

	var d_in1 := (corner1 - p0).normalized()
	var d_out1 := (corner2 - corner1).normalized()
	var d_in2 := d_out1
	var d_out2 := (p3 - corner2).normalized()

	var points := PackedVector2Array()
	points.append(p0)
	points.append_array(_build_elbow_points(corner1, d_in1, d_out1, r))
	points.append_array(_build_elbow_points(corner2, d_in2, d_out2, r))
	points.append(p3)
	return points


func _build_u_route(
		p0: Vector2,
		dir0: Vector2,
		p3: Vector2,
		_dir3: Vector2,
		radius: float,
		unit_px: float,
) -> PackedVector2Array:
	var extension := 2.0 * unit_px
	var corner1: Vector2
	var corner2: Vector2

	if dir0.x > 0.0:
		var ext_x := maxf(p0.x, p3.x) + extension
		corner1 = Vector2(ext_x, p0.y)
		corner2 = Vector2(ext_x, p3.y)
	elif dir0.x < 0.0:
		var ext_x := minf(p0.x, p3.x) - extension
		corner1 = Vector2(ext_x, p0.y)
		corner2 = Vector2(ext_x, p3.y)
	elif dir0.y > 0.0:
		var ext_y := maxf(p0.y, p3.y) + extension
		corner1 = Vector2(p0.x, ext_y)
		corner2 = Vector2(p3.x, ext_y)
	else:
		var ext_y := minf(p0.y, p3.y) - extension
		corner1 = Vector2(p0.x, ext_y)
		corner2 = Vector2(p3.x, ext_y)

	var seg1_len := p0.distance_to(corner1)
	var seg_mid_len := corner1.distance_to(corner2)
	var seg3_len := corner2.distance_to(p3)
	var r := minf(radius, minf(seg1_len, minf(seg_mid_len * 0.5, seg3_len)))

	if r < 1.0:
		return PackedVector2Array([p0, corner1, corner2, p3])

	var d_in1 := (corner1 - p0).normalized()
	var d_out1 := (corner2 - corner1).normalized()
	var d_in2 := d_out1
	var d_out2 := (p3 - corner2).normalized()

	var points := PackedVector2Array()
	points.append(p0)
	points.append_array(_build_elbow_points(corner1, d_in1, d_out1, r))
	points.append_array(_build_elbow_points(corner2, d_in2, d_out2, r))
	points.append(p3)
	return points

# --- Elbow generation ---


func _build_elbow_points(
		corner: Vector2,
		d_in: Vector2,
		d_out: Vector2,
		radius: float,
		point_count: int = 8,
) -> PackedVector2Array:
	var center := corner + (-d_in + d_out) * radius
	var arc_start := corner - d_in * radius
	var arc_end := corner + d_out * radius

	var start_angle := atan2(arc_start.y - center.y, arc_start.x - center.x)
	var end_angle := atan2(arc_end.y - center.y, arc_end.x - center.x)

	var angle_diff := end_angle - start_angle
	if angle_diff > PI:
		angle_diff -= TAU
	elif angle_diff < -PI:
		angle_diff += TAU

	var points := PackedVector2Array()
	for i in point_count:
		var t := float(i) / float(point_count - 1)
		var angle := start_angle + angle_diff * t
		points.append(center + Vector2(cos(angle), sin(angle)) * radius)
	return points

# --- Helpers ---


func _elbow_radius_for_importance(importance: int, unit_px: float) -> float:
	if importance >= 80:
		return 1.0 * unit_px
	elif importance >= 50:
		return 0.75 * unit_px
	else:
		return 0.5 * unit_px


func _bus_side_facing(bus: HudBus, source_pos: Vector2) -> String:
	if bus.orientation == HudBus.ORIENT_VERT:
		var bus_center_x := bus.rect.position.x + bus.rect.size.x * 0.5
		return "W" if source_pos.x < bus_center_x else "E"
	else:
		var bus_center_y := bus.rect.position.y + bus.rect.size.y * 0.5
		return "N" if source_pos.y < bus_center_y else "S"
