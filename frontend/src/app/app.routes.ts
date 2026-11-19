import { Routes } from '@angular/router';
import { Board } from './features/board/board';
import { DayView } from './features/day/day-view';
import { NotesPage } from './features/notes/notes-page';
import { ReportView } from './features/reports/report-view';
import { SettingsPage } from './features/settings/settings-page';

// Route parameters, query parameters and `data` are bound to component inputs
// (withComponentInputBinding): DayView.date, ReportView.mode/date/from/to,
// NotesPage.tag/period/from/to.
export const routes: Routes = [
  { path: '', component: Board, title: 'clockopus' },
  { path: 'day', component: DayView, title: 'Day · clockopus' },
  { path: 'day/:date', component: DayView, title: 'Day · clockopus' },
  { path: 'week', component: ReportView, data: { mode: 'week' }, title: 'Week · clockopus' },
  { path: 'week/:date', component: ReportView, data: { mode: 'week' }, title: 'Week · clockopus' },
  { path: 'month', component: ReportView, data: { mode: 'month' }, title: 'Month · clockopus' },
  {
    path: 'month/:date',
    component: ReportView,
    data: { mode: 'month' },
    title: 'Month · clockopus',
  },
  { path: 'range', component: ReportView, data: { mode: 'range' }, title: 'Range · clockopus' },
  { path: 'notes', component: NotesPage, title: 'Notes · clockopus' },
  { path: 'settings', component: SettingsPage, title: 'Settings · clockopus' },
  { path: '**', redirectTo: '' },
];
