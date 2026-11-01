export class ErrorReport {
  constructor() {
    this.details = document.querySelector('#error-details')
    this.report = document.querySelector('#error-report')
    this.status = document.querySelector('#copy-error-status')
    document.querySelector('#copy-error').addEventListener('click', this.copy)
  }

  clear() {
    this.details.hidden = true
    this.report.hidden = true
    this.report.value = ''
    this.status.textContent = ''
  }

  show(message, stack) {
    this.clear()

    if (!message) return

    this.details.hidden = false
    this.report.value = JSON.stringify(
      {
        message,
        stack,
        browser: navigator.userAgent,
        platform: navigator.platform,
        page: location.origin + location.pathname,
        date: new Date().toISOString(),
      },
      null,
      2,
    )
  }

  copy = async () => {
    try {
      await navigator.clipboard.writeText(this.report.value)
      this.status.textContent = 'Copied. Paste these details with your bug report.'
    } catch {
      this.report.hidden = false
      this.report.focus()
      this.report.select()
      this.status.textContent = 'Copy the selected details below.'
    }
  }
}
