// The header's dataset picker: each option's value is the page it opens.
// <xfina-select> only reports the choice; what a choice does is this site's
// decision.
document.addEventListener("change", (event) => {
  if (event.target.matches?.("xfina-select[data-navigate]")) {
    location.href = event.detail.value;
  }
});
