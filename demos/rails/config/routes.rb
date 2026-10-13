Rails.application.routes.draw do
  # Reveal health status on /up that returns 200 if the app boots with no exceptions, otherwise 500.
  # Can be used by load balancers and uptime monitors to verify that the app is live.
  get "up" => "rails/health#show", as: :rails_health_check

  root "samples#index"

  resources :invoices, only: %i[index show]

  resource :playground, only: :show, controller: :playground do
    post "/", action: :render_pdf, defaults: { format: :json }
  end

  resources :samples, only: %i[index show] do
    get "files/*path", to: "samples#file", as: :file, format: false
  end
end
