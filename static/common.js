'use strict'

function xhr(method, url, cb, etag) {
	var x = new XMLHttpRequest()
	x.onload = function () {
		cb(x)
	}
	x.onerror = function () {
		cb(x)
	}
	x.open(method, url, true)
	if (etag) x.setRequestHeader('If-None-Match', etag)
	x.send(null)
}

function addEvent(element, name, handler) {
	if (element.addEventListener) {
		element.addEventListener(name, handler, false)
	} else if (element.attachEvent) {
		element.attachEvent('on' + name, handler)
	}
}
