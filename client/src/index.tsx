/* @refresh reload */
import { render } from 'solid-js/web';
import '@unocss/reset/tailwind.css';
import 'virtual:uno.css';
import './styles/accessibility.css';
import './styles/components.css';
import App from './App';

const root = document.getElementById('root');

if (!root) {
  throw new Error('Root element not found');
}

render(() => <App />, root);
