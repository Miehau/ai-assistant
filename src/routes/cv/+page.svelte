<script>
    export let jobs = [
      { position: "Senior Software Engineer", company: "Tech Corp", duration: "Jan 2020 - Present", description: "..." },
      { position: "Software Engineer", company: "Web Solutions", duration: "May 2015 - Dec 2019", description: "..." },
      // Add more job experiences as needed
    ];
  
    export let education = [
      { qualification: "BSc Computer Science", institution: "University of London", duration: "2010 - 2014" },
      // Add more educational qualifications as needed
    ];
  
    export let skills = ["JavaScript", "Svelte", "Tailwind CSS", "Node.js"];
    export let languages = ["English", "German"];
  </script>
  
  <style>
    @media print {
      @page {
        margin: 0;
      }
      body {
        margin: 1cm;
      }
    }
  </style>
  
  <div class="max-w-4xl mx-auto bg-white shadow-lg p-8 rounded-lg grid grid-cols-2 gap-6">
    <!-- Left Column: Personal Details & Interests -->
    <div class="space-y-6">
      <section>
        <h1 class="text-3xl font-bold text-blue-500">Your Name</h1>
        <p class="text-gray-600">Senior Software Engineer</p>
      </section>
      <section class="space-y-2">
        <h2 class="text-xl font-semibold text-gray-700">Contact</h2>
        <p>Email: <a class="text-blue-500" href="mailto:your.email@example.com">your.email@example.com</a></p>
        <p>Phone: <span class="text-blue-500">+44 1234 567890</span></p>
        <p>LinkedIn: <a class="text-blue-500" href="https://linkedin.com/in/yourprofile">Your LinkedIn</a></p>
        <p>GitHub: <a class="text-blue-500" href="https://github.com/yourprofile">github.com/yourprofile</a></p>
      </section>
      <section class="space-y-2">
        <h2 class="text-xl font-semibold text-gray-700">Skills</h2>
        <ul class="list-disc list-inside">
          {#each skills as skill}
            <li class="text-gray-600">{skill}</li>
          {/each}
        </ul>
      </section>
      <section class="space-y-2">
        <h2 class="text-xl font-semibold text-gray-700">Languages</h2>
        <ul class="list-disc list-inside">
          {#each languages as language}
            <li class="text-gray-600">{language}</li>
          {/each}
        </ul>
      </section>
    </div>
  
    <!-- Right Column: Work Experience & Education -->
    <div class="space-y-6">
      <section class="space-y-2">
        <h2 class="text-xl font-semibold text-gray-700">Profile</h2>
        <p class="text-gray-600">Accomplished and dedicated Senior Software Engineer with over 10 years of experience in developing scalable web applications...</p>
      </section>
      <section class="space-y-2">
        <h2 class="text-xl font-semibold text-gray-700">Work Experience</h2>
        {#each jobs as job}
          <div>
            <h3 class="text-lg font-bold text-blue-500">{job.position}</h3>
            <p class="text-gray-600">{job.company} - {job.duration}</p>
            <p class="text-gray-600">{job.description}</p>
          </div>
        {/each}
      </section>
      <section class="space-y-2">
        <h2 class="text-xl font-semibold text-gray-700">Education</h2>
        {#each education as edu}
          <div>
            <h3 class="text-lg font-bold text-blue-500">{edu.qualification}</h3>
            <p class="text-gray-600">{edu.institution} - {edu.duration}</p>
          </div>
        {/each}
      </section>
    </div>
  </div>
