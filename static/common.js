'use strict'

function xhr(method, url, cb) {
	var x = new XMLHttpRequest()
	x.onload = function () {
		cb(x)
	}
	x.onerror = function () {
		cb(x)
	}
	x.open(method, url, true)
	x.send(null)
}
