// Turks: opens a regular DSM window with the Turks page inside.
Ext.ns("Turks");

Ext.define("Turks.AppInstance", {
	extend: "SYNO.SDS.AppInstance",
	appWindowName: "Turks.AppWindow"
});

Ext.define("Turks.AppWindow", {
	extend: "SYNO.SDS.AppWindow",

	constructor: function(config) {
		this.appInstance = config.appInstance;
		config = Ext.apply({
			resizable: true,
			maximizable: true,
			minimizable: true,
			width: 1280,
			height: 820,
			minWidth: 1000,
			minHeight: 660,
			items: [{
				xtype: "box",
				autoEl: {
					tag: "iframe",
					src: "/webman/3rdparty/Turks/index.html",
					width: "100%",
					height: "100%",
					frameborder: "0",
					style: "background:#04040c"
				}
			}]
		}, config);
		this.callParent([config]);
	}
});
