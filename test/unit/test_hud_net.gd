extends GutTest

func test_create_direct_arc():
	var net := HudNet.create(
		"sel_to_status",
		HudNet.KIND_DATAFLOW,
		70,
		"stack.selection",
		"status.selection",
	)
	assert_eq(net.net_id, "sel_to_status")
	assert_eq(net.kind, HudNet.KIND_DATAFLOW)
	assert_eq(net.importance, 70)
	assert_eq(net.from_port_id, "stack.selection")
	assert_eq(net.to_port_id, "status.selection")
	assert_eq(net.routing_mode, "DIRECT_ARC")
	assert_eq(net.bus_id, "")


func test_create_bus_branch():
	var net := HudNet.create(
		"active_to_focus",
		HudNet.KIND_HIGHLIGHT,
		90,
		"stack.active",
		"inspector.focus",
		"BUS_BRANCH",
		"center_bus",
	)
	assert_eq(net.routing_mode, "BUS_BRANCH")
	assert_eq(net.bus_id, "center_bus")


func test_visual_class_primary():
	var net := HudNet.create("n", "k", 90, "a", "b")
	assert_eq(net.get_visual_class(), "primary")


func test_visual_class_primary_boundary():
	var net := HudNet.create("n", "k", 80, "a", "b")
	assert_eq(net.get_visual_class(), "primary")


func test_visual_class_secondary():
	var net := HudNet.create("n", "k", 60, "a", "b")
	assert_eq(net.get_visual_class(), "secondary")


func test_visual_class_secondary_boundary():
	var net := HudNet.create("n", "k", 50, "a", "b")
	assert_eq(net.get_visual_class(), "secondary")


func test_visual_class_tertiary():
	var net := HudNet.create("n", "k", 30, "a", "b")
	assert_eq(net.get_visual_class(), "tertiary")


func test_visual_class_tertiary_boundary():
	var net := HudNet.create("n", "k", 49, "a", "b")
	assert_eq(net.get_visual_class(), "tertiary")
