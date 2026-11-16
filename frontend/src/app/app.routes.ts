import { Routes } from '@angular/router';
import { Board } from './features/board/board';

export const routes: Routes = [
  { path: '', component: Board, title: 'clockopus' },
  { path: '**', redirectTo: '' },
];
