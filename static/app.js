'use strict'

var uploadform = document.getElementById('uploadform')
var uploadstatus = document.getElementById('uploadstatus')
var fileinput = document.getElementById('fileinput')
var fileinfo = document.getElementById('fileinfo')
var booksNode = document.getElementById('books')
var shelfstatus = document.getElementById('shelfstatus')
var flashtimer = null
var shelfetag = null
var polltimer = null
var shelfexpired = false

function hideUploadStatus() {
	uploadstatus.style.opacity = 0
	clearTimeout(flashtimer)
	flashtimer = setTimeout(function () {
		uploadstatus.textContent = ''
		uploadstatus.className = ''
	}, 500)
}

function handleFlash(flash) {
	clearTimeout(flashtimer)
	if (!flash) { hideUploadStatus(); return }
	uploadstatus.className = flash.success ? 'success' : 'error'
	uploadstatus.textContent = flash.message.trim()
	uploadstatus.style.opacity = 1
}

function fileinputChange () {
	var file = fileinput.files[0]
	if (!file) { fileinfo.textContent = ''; return }
	var lowername = file.name.toLowerCase()
	if (lowername.length < 5 || lowername.lastIndexOf('.epub') !== lowername.length - 5) {
		alert('Choose an EPUB file.')
		fileinput.value = ''
		fileinfo.textContent = ''
		return
	}
	fileinfo.textContent = 'EPUB selected'
}

function showShelfUnavailable () {
	shelfexpired = true
	clearTimeout(polltimer)
	shelfstatus.className = 'shelf-status error'
	shelfstatus.textContent = 'This shelf is unavailable or expired.'
	uploadform.style.display = 'none'
	booksNode.innerHTML = '<p class="empty">Create a new shelf to continue.</p>'
}

function showExpiration (expiresAt) {
	if (!expiresAt) return
	shelfstatus.className = 'shelf-status'
	shelfstatus.textContent = 'Expires after inactivity: ' + expiresAt.replace('T', ' ').replace('Z', ' UTC')
}

function renderBooks (books) {
	booksNode.innerHTML = ''
	if (!books.length) {
		booksNode.innerHTML = '<p class="empty">No books yet. Upload an EPUB from either joined device.</p>'
		return
	}
	for (var i = 0; i < books.length; i += 1) {
		(function (book) {
			var row = document.createElement('div')
			row.className = 'book-row'
			var title = document.createElement('div')
			title.className = 'book-title'
			title.textContent = book.title || book.filename
			var author = document.createElement('div')
			author.className = 'book-author'
			author.textContent = book.author || ''
			var remove = document.createElement('button')
			remove.type = 'button'
			remove.className = 'danger-button'
			remove.textContent = 'Delete'
			remove.onclick = function () {
				if (!confirm('Delete ' + (book.title || 'this book') + '?')) return
				xhr('DELETE', 'api/books/' + encodeURIComponent(book.id), function (req) {
					if (req.status === 200) { shelfetag = null; loadBooks() }
					else if (req.status === 404) showShelfUnavailable()
					else handleFlash({ success: false, message: req.responseText || 'Delete failed.' })
				})
			}
			var download = document.createElement('a')
			download.className = 'action-button'
			download.href = book.downloadUrl
			download.textContent = 'Download'
			var actions = document.createElement('div')
			actions.className = 'book-actions'
			actions.appendChild(download)
			actions.appendChild(remove)
			var details = document.createElement('div')
			details.className = 'book-details'
			details.appendChild(title)
			if (book.author) details.appendChild(author)
			row.appendChild(details)
			row.appendChild(actions)
			booksNode.appendChild(row)
		})(books[i])
	}
}

function loadBooks (done) {
	if (shelfexpired) { if (done) done(); return }
	xhr('GET', 'api/books', function (req) {
		if (req.status === 304) { if (done) done(); return }
		if (req.status === 404) { showShelfUnavailable(); if (done) done(); return }
		if (req.status !== 200) {
			shelfstatus.className = 'shelf-status error'
			shelfstatus.textContent = 'Unable to refresh this shelf. Retrying...'
			if (done) done()
			return
		}
		var snapshot = null
		try { snapshot = JSON.parse(req.responseText) } catch (err) {}
		if (!snapshot || !snapshot.books) {
			shelfstatus.className = 'shelf-status error'
			shelfstatus.textContent = 'The shelf response could not be read.'
			if (done) done()
			return
		}
		shelfetag = req.getResponseHeader('ETag')
		showExpiration(snapshot.expiresAt)
		renderBooks(snapshot.books)
		if (done) done()
	}, shelfetag)
}

function pollBooks () {
	loadBooks(function () { if (!shelfexpired) polltimer = setTimeout(pollBooks, 5000) })
}

addEvent(uploadstatus, 'click', hideUploadStatus)
addEvent(fileinput, 'change', fileinputChange)
addEvent(uploadform, 'submit', function (e) {
	e = e || window.event
	if (e.preventDefault) e.preventDefault()
	e.returnValue = false
	hideUploadStatus()
	var fd = new FormData(uploadform)
	var req = new XMLHttpRequest()
	req.open('POST', uploadform.action, true)
	req.upload.onprogress = function (e) {
		if (e.lengthComputable) {
			var complete = e.loaded / e.total === 1
			handleFlash({ success: true, message: complete ? 'Processing EPUB...' : 'Uploading... ' + Math.round(100 * e.loaded / e.total) + '%' })
		}
	}
	req.onload = function () {
		handleFlash({ success: req.status === 200, message: req.responseText })
		if (req.status === 200) { uploadform.reset(); fileinfo.textContent = ''; shelfetag = null; loadBooks() }
		else if (req.status === 404) showShelfUnavailable()
	}
	req.onerror = function () { handleFlash({ success: false, message: 'Upload failed.' }) }
	req.send(fd)
	return false
})

loadBooks()
polltimer = setTimeout(pollBooks, 5000)
addEvent(window, 'focus', loadBooks)
