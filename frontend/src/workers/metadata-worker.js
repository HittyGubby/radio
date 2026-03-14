const INFO_URL = "/info";

self.onmessage = async function(e) {
  const { type } = e.data;

  if (type === 'fetch') {
    try {
      const response = await fetch(INFO_URL);
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      const data = await response.json();
      self.postMessage({ type: 'success', data });
    } catch (error) {
      self.postMessage({ type: 'error', error: error.message });
    }
  }
};