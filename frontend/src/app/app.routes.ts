import { Routes } from '@angular/router';
import { Board } from './features/board/board';
import { DayView } from './features/day/day-view';

export const routes: Routes = [
  { path: '', component: Board, title: 'clockopus' },
  // `:date` is bound to DayView's `date` input (withComponentInputBinding).
  { path: 'day', component: DayView, title: 'Day · clockopus' },
  { path: 'day/:date', component: DayView, title: 'Day · clockopus' },
  { path: '**', redirectTo: '' },
];
